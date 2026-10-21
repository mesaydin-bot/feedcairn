export type KeyAction =
  | "next"
  | "prev"
  | "read"
  | "star"
  | "open"
  | "refresh"
  | "search"
  | "help";

/** Classic RSS-reader keymap: j/k/n/p/o/v/m/s (plus arrows, r, /, ?). */
export function keyAction(key: string, typing: boolean): KeyAction | null {
  if (typing) return null;
  switch (key) {
    case "j":
    case "n":
    case "ArrowDown":
      return "next";
    case "k":
    case "p":
    case "ArrowUp":
      return "prev";
    case "m":
      return "read";
    case "s":
      return "star";
    case "o":
    case "v":
      return "open";
    case "r":
      return "refresh";
    case "/":
      return "search";
    case "?":
      return "help";
    default:
      return null;
  }
}
