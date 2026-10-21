use crate::error::{Error, Result};
use crate::models::ImportResult;
use rusqlite::{Connection, OptionalExtension};
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpmlOutline {
    pub text: String,
    pub title: Option<String>,
    pub xml_url: Option<String>,
    pub html_url: Option<String>,
    pub children: Vec<OpmlOutline>,
}

impl OpmlOutline {
    pub fn display_name(&self) -> &str {
        self.title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(self.text.as_str())
    }

    pub fn is_feed(&self) -> bool {
        self.xml_url
            .as_deref()
            .map(|u| !u.trim().is_empty())
            .unwrap_or(false)
    }
}

pub fn parse_opml(xml: &str) -> Result<Vec<OpmlOutline>> {
    let xml = strip_bom(xml)
        .replacen("encoding=\"UTF-8\"", "", 1)
        .replacen("encoding='UTF-8'", "", 1)
        .replacen("encoding=\"utf-8\"", "", 1)
        .replacen("encoding=\"utf-8\"", "", 1);
    let doc = roxmltree::Document::parse(&xml)?;
    let body = doc
        .descendants()
        .find(|n| n.has_tag_name("body"))
        .ok_or_else(|| Error::msg("OPML file is missing a <body> element"))?;
    Ok(body
        .children()
        .filter(|n| n.is_element() && n.has_tag_name("outline"))
        .map(parse_outline)
        .collect())
}

fn parse_outline(node: roxmltree::Node) -> OpmlOutline {
    let attr = |names: &[&str]| {
        for name in names {
            if let Some(v) = node.attribute(*name) {
                return Some(v.to_string());
            }
            // OPML attributes are sometimes mixed-case
            if let Some(v) = node.attributes().find(|a| a.name().eq_ignore_ascii_case(name)) {
                return Some(v.value().to_string());
            }
        }
        None
    };
    let text = attr(&["text", "title"]).unwrap_or_default();
    let title = attr(&["title"]);
    OpmlOutline {
        text,
        title,
        xml_url: attr(&["xmlUrl", "xmlurl"]),
        html_url: attr(&["htmlUrl", "htmlurl"]),
        children: node
            .children()
            .filter(|n| n.is_element() && n.has_tag_name("outline"))
            .map(parse_outline)
            .collect(),
    }
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

pub fn import_opml(conn: &Connection, xml: &str) -> Result<ImportResult> {
    let outlines = parse_opml(xml)?;
    let mut result = ImportResult {
        folders_created: 0,
        feeds_added: 0,
        feeds_skipped: 0,
    };
    import_children(conn, &outlines, None, &mut result)?;
    Ok(result)
}

fn import_children(
    conn: &Connection,
    outlines: &[OpmlOutline],
    parent_folder: Option<i64>,
    result: &mut ImportResult,
) -> Result<()> {
    for outline in outlines {
        if outline.is_feed() {
            let url = outline.xml_url.as_deref().unwrap().trim();
            let title = outline.display_name();
            let title = if title.trim().is_empty() { url } else { title };
            let (feed, created) = crate::db::add_feed(
                conn,
                url,
                title,
                outline.html_url.as_deref(),
                parent_folder,
            )?;
            if created {
                result.feeds_added += 1;
            } else {
                result.feeds_skipped += 1;
                // If the feed already existed unfiled, move it into the OPML folder.
                if feed.folder_id.is_none() && parent_folder.is_some() {
                    crate::db::update_feed(conn, feed.id, None, Some(parent_folder))?;
                }
            }
            // Nested outlines under a feed are unusual; treat them as siblings in the same folder.
            if !outline.children.is_empty() {
                import_children(conn, &outline.children, parent_folder, result)?;
            }
        } else {
            let name = outline.display_name();
            let name = if name.trim().is_empty() {
                "Untitled folder"
            } else {
                name
            };
            let existed = conn
                .query_row(
                    "SELECT id FROM folders WHERE name = ?1 AND parent_id IS ?2",
                    rusqlite::params![name, parent_folder],
                    |r| r.get::<_, i64>(0),
                )
                .optional()?
                .is_some();
            let folder_id = crate::db::find_or_create_folder(conn, name, parent_folder)?;
            if !existed {
                result.folders_created += 1;
            }
            import_children(conn, &outline.children, Some(folder_id), result)?;
        }
    }
    Ok(())
}

pub fn export_opml(conn: &Connection) -> Result<String> {
    let bootstrap = crate::db::bootstrap(conn)?;
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head>
    <title>Feedcairn export</title>
  </head>
  <body>
"#,
    );
    for node in &bootstrap.folders {
        write_folder(&mut out, node, 2)?;
    }
    for feed in &bootstrap.root_feeds {
        write_feed(&mut out, feed, 2)?;
    }
    out.push_str("  </body>\n</opml>\n");
    Ok(out)
}

fn write_folder(out: &mut String, node: &crate::models::FolderNode, indent: usize) -> Result<()> {
    let pad = "  ".repeat(indent);
    writeln!(
        out,
        r#"{pad}<outline text="{}" title="{}">"#,
        xml_escape(&node.folder.name),
        xml_escape(&node.folder.name)
    )
    .map_err(|e| Error::msg(e.to_string()))?;
    for child in &node.children {
        write_folder(out, child, indent + 1)?;
    }
    for feed in &node.feeds {
        write_feed(out, feed, indent + 1)?;
    }
    writeln!(out, "{pad}</outline>").map_err(|e| Error::msg(e.to_string()))?;
    Ok(())
}

fn write_feed(out: &mut String, feed: &crate::models::Feed, indent: usize) -> Result<()> {
    let pad = "  ".repeat(indent);
    let html = feed
        .site_url
        .as_deref()
        .map(|u| format!(r#" htmlUrl="{}""#, xml_escape(u)))
        .unwrap_or_default();
    writeln!(
        out,
        r#"{pad}<outline type="rss" text="{}" title="{}" xmlUrl="{}"{html} />"#,
        xml_escape(&feed.title),
        xml_escape(&feed.title),
        xml_escape(&feed.url)
    )
    .map_err(|e| Error::msg(e.to_string()))?;
    Ok(())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="1.0">
  <head><title>Reader export</title></head>
  <body>
    <outline text="News" title="News">
      <outline text="BBC" title="BBC" type="rss" xmlUrl="https://feeds.bbci.co.uk/news/rss.xml" htmlUrl="https://www.bbc.com/news" />
      <outline text="World" title="World">
        <outline text="Reuters" xmlUrl="https://www.reutersagency.com/feed/" />
      </outline>
    </outline>
    <outline text="Naked feed" xmlUrl="https://hnrss.org/frontpage" htmlUrl="https://news.ycombinator.com/" />
  </body>
</opml>
"#;

    #[test]
    fn parses_nested_reader_export_opml() {
        let tree = parse_opml(SAMPLE).unwrap();
        assert_eq!(tree.len(), 2);
        assert!(!tree[0].is_feed());
        assert_eq!(tree[0].display_name(), "News");
        assert_eq!(tree[0].children.len(), 2);
        assert!(tree[0].children[0].is_feed());
        assert_eq!(
            tree[0].children[0].xml_url.as_deref(),
            Some("https://feeds.bbci.co.uk/news/rss.xml")
        );
        assert_eq!(tree[0].children[1].display_name(), "World");
        assert_eq!(tree[0].children[1].children.len(), 1);
        assert!(tree[1].is_feed());
    }

    #[test]
    fn import_preserves_folder_hierarchy_and_skips_dupes() {
        let conn = db::open_memory().unwrap();
        let first = import_opml(&conn, SAMPLE).unwrap();
        assert_eq!(first.folders_created, 2);
        assert_eq!(first.feeds_added, 3);
        assert_eq!(first.feeds_skipped, 0);

        let second = import_opml(&conn, SAMPLE).unwrap();
        assert_eq!(second.feeds_added, 0);
        assert_eq!(second.feeds_skipped, 3);

        let boot = db::bootstrap(&conn).unwrap();
        assert_eq!(boot.folders.len(), 1);
        assert_eq!(boot.folders[0].folder.name, "News");
        assert_eq!(boot.folders[0].feeds.len(), 1);
        assert_eq!(boot.folders[0].children.len(), 1);
        assert_eq!(boot.folders[0].children[0].folder.name, "World");
        assert_eq!(boot.folders[0].children[0].feeds.len(), 1);
        assert_eq!(boot.root_feeds.len(), 1);
        assert_eq!(boot.root_feeds[0].url, "https://hnrss.org/frontpage");
    }

    #[test]
    fn export_roundtrip_keeps_folders() {
        let conn = db::open_memory().unwrap();
        import_opml(&conn, SAMPLE).unwrap();
        let xml = export_opml(&conn).unwrap();
        assert!(xml.contains("xmlUrl=\"https://feeds.bbci.co.uk/news/rss.xml\""));
        assert!(xml.contains("text=\"World\""));
        let conn2 = db::open_memory().unwrap();
        let imported = import_opml(&conn2, &xml).unwrap();
        assert_eq!(imported.feeds_added, 3);
        assert_eq!(imported.folders_created, 2);
    }
}
