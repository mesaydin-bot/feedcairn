import { useCallback, useEffect, useState } from "react";

export const READER_FONT_SIZE_KEY = "feedcairn.reader.fontSize";
export const READER_FONT_KEY = "feedcairn.reader.font";

/** Discrete reading sizes in px (applied to article body). */
export const FONT_SIZES = [14, 15, 16, 17, 18, 19, 20, 21, 22] as const;
export type ReaderFontSize = (typeof FONT_SIZES)[number];
export const DEFAULT_FONT_SIZE: ReaderFontSize = 18;

export const READER_FONTS = [
  {
    id: "system",
    label: "Sistem",
    stack: '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
  },
  {
    id: "newyork",
    label: "New York",
    stack: '"New York", "Iowan Old Style", "Apple Garamond", Georgia, serif',
  },
  {
    id: "georgia",
    label: "Georgia",
    stack: 'Charter, Georgia, "Times New Roman", serif',
  },
  {
    id: "palatino",
    label: "Palatino",
    stack: '"Iowan Old Style", Palatino, "Palatino Linotype", serif',
  },
  {
    id: "avenir",
    label: "Avenir",
    stack: '"Avenir Next", "Helvetica Neue", sans-serif',
  },
] as const;

export type ReaderFontId = (typeof READER_FONTS)[number]["id"];
export const DEFAULT_FONT_ID: ReaderFontId = "newyork";

function parseFontSize(raw: string | null): ReaderFontSize {
  const n = Number(raw);
  if (FONT_SIZES.includes(n as ReaderFontSize)) return n as ReaderFontSize;
  return DEFAULT_FONT_SIZE;
}

function parseFontId(raw: string | null): ReaderFontId {
  if (READER_FONTS.some((f) => f.id === raw)) return raw as ReaderFontId;
  return DEFAULT_FONT_ID;
}

export function fontStackFor(id: ReaderFontId): string {
  return READER_FONTS.find((f) => f.id === id)?.stack ?? READER_FONTS[0].stack;
}

export function useReaderTypography() {
  const [fontSize, setFontSizeState] = useState<ReaderFontSize>(() => {
    try {
      return parseFontSize(localStorage.getItem(READER_FONT_SIZE_KEY));
    } catch {
      return DEFAULT_FONT_SIZE;
    }
  });
  const [fontId, setFontIdState] = useState<ReaderFontId>(() => {
    try {
      return parseFontId(localStorage.getItem(READER_FONT_KEY));
    } catch {
      return DEFAULT_FONT_ID;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(READER_FONT_SIZE_KEY, String(fontSize));
    } catch {
      /* ignore */
    }
  }, [fontSize]);

  useEffect(() => {
    try {
      localStorage.setItem(READER_FONT_KEY, fontId);
    } catch {
      /* ignore */
    }
  }, [fontId]);

  const setFontSize = useCallback((size: ReaderFontSize) => {
    setFontSizeState(size);
  }, []);

  const setFontId = useCallback((id: ReaderFontId) => {
    setFontIdState(id);
  }, []);

  const bumpSize = useCallback((delta: -1 | 1) => {
    setFontSizeState((cur) => {
      const idx = FONT_SIZES.indexOf(cur);
      const next = FONT_SIZES[Math.min(FONT_SIZES.length - 1, Math.max(0, idx + delta))];
      return next ?? cur;
    });
  }, []);

  return {
    fontSize,
    fontId,
    fontStack: fontStackFor(fontId),
    setFontSize,
    setFontId,
    bumpSize,
    canSmaller: fontSize > FONT_SIZES[0],
    canLarger: fontSize < FONT_SIZES[FONT_SIZES.length - 1],
  };
}
