// Import is always reviewed by the user; no clipboard polling or automatic requests.
export function importableLinks(text: string): string {
  if (new TextEncoder().encode(text).length > 1_048_576) return "";
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
  if (lines.length > 1000) return "";
  const links = lines.filter((line) => {
    if (line.length > 16384 || /\s/.test(line)) return false;
    try {
      const url = new URL(line);
      return (
        ["http:", "https:"].includes(url.protocol) &&
        !url.username &&
        !url.password
      );
    } catch {
      return false;
    }
  });
  return [...new Set(links)].join("\n");
}
