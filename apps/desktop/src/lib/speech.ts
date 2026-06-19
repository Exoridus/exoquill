// Read-aloud via the webview's Web Speech API (system voices). This is the
// interim TTS until the bundled Piper provider lands (decisions D2); it needs no
// native binary and gives real German/English speech today.

interface SpeakOptions {
  lang?: string;
  rate?: number;
  onEnd?: () => void;
}

export function speak(text: string, opts: SpeakOptions = {}): void {
  const synth = window.speechSynthesis;
  if (!synth || !text.trim()) {
    opts.onEnd?.();
    return;
  }
  synth.cancel();
  const utterance = new SpeechSynthesisUtterance(text);
  utterance.lang = opts.lang ?? "de-DE";
  utterance.rate = opts.rate ?? 1;
  utterance.onend = () => opts.onEnd?.();
  utterance.onerror = () => opts.onEnd?.();
  synth.speak(utterance);
}

export function stopSpeaking(): void {
  window.speechSynthesis?.cancel();
}

export function isSpeechSupported(): boolean {
  return typeof window !== "undefined" && "speechSynthesis" in window;
}
