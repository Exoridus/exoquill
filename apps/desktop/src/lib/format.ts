// Split long Markdown into chunks the formatter can handle in one pass. The
// per-call model has a bounded context/output, so a whole long note must be
// formatted in pieces (and reassembled) instead of one giant — truncating —
// request. Chunks never split a fenced code block or a table mid-way; blocks are
// packed up to a character budget.

/** Group lines into blocks separated by blank lines, keeping fenced code blocks
 *  (``` … ```) whole even when they contain blank lines. */
function splitBlocks(text: string): string[] {
  const blocks: string[] = [];
  let buf: string[] = [];
  let inFence = false;
  const flush = () => {
    if (buf.length) {
      blocks.push(buf.join("\n"));
      buf = [];
    }
  };
  for (const line of text.split("\n")) {
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      buf.push(line);
      continue;
    }
    if (!inFence && line.trim() === "") {
      flush();
    } else {
      buf.push(line);
    }
  }
  flush();
  return blocks;
}

/**
 * Pack Markdown blocks into chunks no larger than `budget` characters (a single
 * oversized block — e.g. a huge table — is emitted alone rather than split). The
 * default budget leaves room for the system prompt + the model's reply within a
 * 4k-token context.
 */
export function chunkMarkdown(text: string, budget = 3500): string[] {
  const chunks: string[] = [];
  let current = "";
  for (const block of splitBlocks(text)) {
    if (current && current.length + block.length + 2 > budget) {
      chunks.push(current);
      current = "";
    }
    current = current ? `${current}\n\n${block}` : block;
  }
  if (current) chunks.push(current);
  return chunks;
}

/**
 * Clean up dictated/raw text deterministically — no model, so it can never
 * hallucinate, loop, or hang (unlike the small LLM, which did all three). It does
 * the safe, predictable things: applies spoken paragraph commands, drops filler
 * words, normalizes whitespace and the spacing before punctuation, and
 * capitalizes sentence starts. It deliberately does NOT invent sentence
 * boundaries or rephrase — that's the job of the punctuation model (next step);
 * here it relies on punctuation already present (Whisper adds it during dictation).
 */
export function cleanDictation(text: string): string {
  let out = text
    // Spoken paragraph commands → real breaks.
    .replace(/\b(neuer absatz|neue zeile)\b[ .,]*/gi, "\n\n")
    // Drop filler words ("äh", "ähm", "öhm", "ehm"). Can't use `\b` here — it's
    // ASCII-only, so it doesn't see a word boundary before "ä"; match the
    // surrounding boundary chars explicitly instead.
    .replace(/(^|[\s.,!?;:])(äh+|ähm+|öhm+|ehm+)(?=[\s.,!?;:]|$)/gi, "$1")
    // Collapse runs of spaces/tabs.
    .replace(/[ \t]{2,}/g, " ")
    // No space before sentence punctuation.
    .replace(/ +([.,!?;:])/g, "$1")
    // Tidy whitespace around line breaks; at most one blank line.
    .replace(/[ \t]*\n[ \t]*/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  // Capitalize the first letter at the start of the text, after a sentence end,
  // and after a line break. `\p{Ll}` covers German lowercase incl. umlauts.
  out = out.replace(/(^|[.!?]\s|\n+)(\p{Ll})/gu, (_m, sep, ch) => sep + ch.toUpperCase());
  return out;
}
