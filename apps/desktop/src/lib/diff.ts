// Word-level diff for the edit-history view — a small, dependency-free LCS
// (no npm), fine for note-sized inputs.

/** A diff segment: text that's unchanged, added, or removed. */
export interface DiffSegment {
  type: "equal" | "add" | "remove";
  text: string;
}

// Split into words plus the whitespace runs between them, so concatenating the
// tokens back together is lossless (the diff can be rendered inline verbatim).
function tokenize(s: string): string[] {
  return s.match(/\s+|\S+/g) ?? [];
}

/** Word-level diff between `before` and `after` via a longest-common-subsequence
 *  table. Consecutive same-type tokens are coalesced into one segment. */
export function diffWords(before: string, after: string): DiffSegment[] {
  const a = tokenize(before);
  const b = tokenize(after);
  const n = a.length;
  const m = b.length;

  // lcs[i][j] = length of the LCS of a[i..] and b[j..].
  const lcs: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const segments: DiffSegment[] = [];
  const push = (type: DiffSegment["type"], text: string) => {
    const last = segments[segments.length - 1];
    if (last && last.type === type) last.text += text;
    else segments.push({ type, text });
  };

  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      push("equal", a[i]);
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      push("remove", a[i]);
      i++;
    } else {
      push("add", b[j]);
      j++;
    }
  }
  while (i < n) push("remove", a[i++]);
  while (j < m) push("add", b[j++]);
  return segments;
}

/** Added/removed word counts between two strings (for "+x / −y Wörter"). */
export function diffStats(before: string, after: string): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const seg of diffWords(before, after)) {
    if (seg.type === "equal") continue;
    const words = (seg.text.match(/\S+/g) ?? []).length;
    if (seg.type === "add") added += words;
    else removed += words;
  }
  return { added, removed };
}
