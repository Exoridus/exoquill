# ExoQuill — Product & Technical Specification

Version: 0.1 draft
Status: Planning specification
Project type: Open-source local-first desktop application
Primary languages: German and English
Primary platform target: Windows first, then Linux/macOS
Product category: Local AI note capture, dictation, OCR, formatting and read-aloud workspace

---

## 1. Product Summary

**ExoQuill** is a lightweight, local-first desktop application for capturing, cleaning, reading and organizing text from multiple sources.

The application centers around a global note list. All AI-powered tools write into notes:

* Live speech-to-text dictation
* OCR from files, drag and drop, clipboard images and screen regions
* AI-assisted text formatting and correction
* Text-to-speech read-aloud for notes or selected text
* Later: audio file transcription

ExoQuill is not a chatbot, not a full Obsidian clone, not a cloud AI wrapper and not a DAW-like audio tool. It is a focused local writing/capture utility.

---

## 2. Product Positioning

### 2.1 One-sentence pitch

ExoQuill is a private local AI note capture app for dictation, screenshots, formatting and read-aloud workflows.

### 2.2 Short description

ExoQuill lets users dictate notes, extract text from images or screen regions, clean rough text into readable Markdown and listen to notes using local AI models. It is optimized for German and English, runs offline by default and keeps the interface deliberately minimal.

### 2.3 Core product promise

> Speak it, capture it, clean it, read it — locally.

### 2.4 Primary principles

1. **Local-first**

   * No cloud dependency for core features.
   * User data stays on the device.
   * Model downloads are explicit and transparent.

2. **Minimal surface**

   * No complex audio mixer.
   * No excessive model controls in the main UI.
   * Advanced settings exist, but remain out of the way.

3. **Note-centric**

   * Every tool writes into a note.
   * If no note is selected, ExoQuill creates one automatically.
   * Notes are sorted by last edit date.

4. **German/English only**

   * Official support is limited to German and English.
   * Mixed German speech with English technical terms is a first-class use case.
   * Additional languages are not a roadmap priority.

5. **Markdown-native**

   * Notes are stored as Markdown.
   * Formatting actions produce clean Markdown.
   * Export should be simple and transparent.

6. **Trustworthy AI**

   * Raw model output is preserved in history.
   * Formatting should not invent content.
   * Destructive changes should have preview or undo.

---

## 3. Target Users

### 3.1 Primary user

A developer, creator, researcher or power user who wants to quickly capture thoughts, snippets, screenshots and rough spoken input into clean local notes.

### 3.2 Key use cases

#### Use case A — Quick dictation

The user opens ExoQuill, selects or creates a note, clicks Dictate, speaks in German with occasional English technical terms, and the text is appended to the note.

#### Use case B — Background dictation

The user keeps ExoQuill in the tray, triggers dictation with a global shortcut, speaks, and ExoQuill appends the transcript to the active note or creates a new note if none is active.

#### Use case C — OCR from screenshot

The user selects a screen region, ExoQuill extracts text, optionally cleans it, and inserts it into the current note.

#### Use case D — OCR from clipboard

The user copies an image to the clipboard, focuses ExoQuill and presses Ctrl+V. ExoQuill recognizes the clipboard image and inserts extracted text into the active note.

#### Use case E — Quick format

The user selects rough dictated text and clicks a small floating Format button. ExoQuill fixes punctuation, obvious word errors and readability while preserving meaning.

#### Use case F — Custom format

The user selects text and enters an instruction such as: “Mach daraus klare Meeting-Notizen mit Bullet Points.” ExoQuill rewrites only the selected range.

#### Use case G — Read aloud

The user selects a paragraph or whole note and starts read-aloud with a calm local voice.

---

## 4. Non-goals

ExoQuill should explicitly not try to be these things in v0.1/v0.2:

* Full Obsidian replacement
* Full OneNote replacement
* Full Markdown knowledge base
* Cloud sync service
* Team collaboration tool
* Chatbot client
* Audio editing application
* Professional transcription suite
* Multi-language translation tool
* Voice cloning tool
* Meeting bot
* Browser extension
* Mobile app

---

## 5. Feature Overview

### 5.1 Core actions

ExoQuill has four main user-facing actions:

```txt
Dictate
OCR
Format
Read
```

### 5.2 Core object

The central object is always a note.

```txt
Input source → AI provider → result → active note
```

### 5.3 Note targeting rule

When a tool produces text:

```txt
If a note is selected:
  Insert into that note.

If no note is selected:
  Create a new note.
  Insert the result.
  Add it to the note list.
  Sort by updated_at.
```

### 5.4 Insert behavior

Supported insertion modes:

1. Append to end of note
2. Insert at editor cursor
3. Replace selected text
4. Insert below selected text
5. Later: insert into external active application

Default for v0.1:

```txt
Dictation: append to active note
OCR: insert at cursor or append
Format: replace selected text or preview whole-note replacement
Read: no text mutation
```

---

## 6. Version Scope

## 6.1 v0.1 — Local Notes + Basic AI Capture

Goal: usable local alpha.

### Included

#### Notes

* Create note
* Rename note
* Edit note
* Delete note
* Auto-create note when a tool is used without active note
* Notes sorted by last modified date
* Local SQLite persistence
* Markdown editor
* Basic search by title/content

#### Dictation

* Microphone input source dropdown
* Auto gain toggle
* Manual gain slider when auto gain is disabled
* Small input level meter
* Start/stop dictation
* Append transcription to selected note
* German default with English technical terms
* Local Whisper provider
* Basic session limit
* No raw audio retention by default

#### OCR

* File picker image OCR
* Drag and drop image OCR
* Clipboard image paste via Ctrl+V
* German + English OCR
* Basic image preprocessing
* Insert OCR result into active note
* Optional quick format after OCR

#### Formatting

* Quick Format for selected text
* Custom instruction input for selected text
* Quick Format for whole note with preview
* Custom instruction for whole note with preview
* Local LLM formatter provider
* Undo support

#### Read aloud

* Read selected text
* Read whole note
* Read from cursor
* Play/pause/stop
* Voice selector
* Speed control
* Local TTS provider

#### App shell

* Tauri desktop shell
* System tray
* Basic settings
* Local model manager
* Model download screen
* Privacy-first onboarding
* Windows packaging

### Excluded from v0.1

* Audio file transcription
* Cloud sync
* Plugin system
* External app text insertion
* Team sharing
* Mobile app
* Full screen region OCR if implementation risk is too high
* Advanced document layout OCR
* Multi-speaker diarization

---

## 6.2 v0.2 — Screen Capture + Background Workflow

Goal: make the app feel like a real utility.

### Included

* Screen region OCR overlay
* Global shortcut for dictation
* Global shortcut for OCR region
* Global shortcut for quick note
* Background dictation mode
* Improved VAD segmentation
* Dictation history view
* Note event history view
* Better model settings
* Export Markdown
* Export selected notes
* Better read-aloud queue
* Better floating selection toolbar

---

## 6.3 v0.3 — Power Features

Goal: improve quality, scale and extensibility.

### Included

* Audio file transcription
* Batch OCR
* Better OCR provider
* Optional faster-whisper provider
* Optional alternative TTS provider
* User dictionary / custom terms
* Per-note language mode
* Per-note formatting style
* Local backup/export
* Import Markdown files
* Basic plugin/provider architecture hardening

---

## 7. User Interface Specification

## 7.1 Main layout

Desktop window:

```txt
┌─────────────────────────────────────────────────────────────┐
│ Top Bar                                                     │
│ ExoQuill        [Dictate] [OCR] [Format] [Read] [Settings]  │
├───────────────────────┬─────────────────────────────────────┤
│ Notes Sidebar          │ Editor                              │
│                        │                                     │
│ Search notes           │ Note title                           │
│                        │                                     │
│ Today                  │ Markdown/WYSIWYG editor              │
│ - Note A               │                                     │
│ - Note B               │                                     │
│                        │                                     │
│ Yesterday              │                                     │
│ - Note C               │                                     │
├───────────────────────┴─────────────────────────────────────┤
│ Status: Local models ready · German + English                │
└─────────────────────────────────────────────────────────────┘
```

## 7.2 Visual style

Direction:

* Calm
* Minimal
* Warm technical
* Docs-friendly
* Low visual noise
* No “AI dashboard” look
* No enterprise bloat
* No heavy gradients

Suggested visual language:

* Dark and light mode
* Muted off-white/editor surfaces
* Ink/graphite typography
* Small accent color from Exo ecosystem
* Subtle quill/ink motif
* Compact toolbar
* Strong empty states

## 7.3 Main actions

Top-level action buttons:

```txt
Dictate
OCR
Format
Read
```

Each action should be available in three places where relevant:

1. Top toolbar
2. Selection floating toolbar
3. Global shortcut / tray menu

---

# 8. Notes UX

## 8.1 Sidebar

The sidebar shows notes sorted by `updated_at DESC`.

Each note item shows:

* Title
* Short preview
* Last modified time
* Optional source badge:

  * Dictation
  * OCR
  * Formatted
  * Imported
  * Mixed

## 8.2 Note title behavior

Default title generation:

```txt
If first meaningful line exists:
  Use first line as title.

Else:
  Use "Untitled Note".

If note was created by OCR:
  Use "OCR Note – YYYY-MM-DD HH:mm".

If note was created by dictation:
  Use "Dictation – YYYY-MM-DD HH:mm".
```

## 8.3 Empty state

If no note exists:

```txt
Start with a note, dictate something, OCR an image, or paste a screenshot.
```

Buttons:

```txt
New Note
Start Dictation
OCR Image
Paste Image
```

## 8.4 Editor

The editor should support:

* Markdown shortcuts
* Basic WYSIWYG-like editing
* Selection handling
* Floating selection toolbar
* Undo/redo
* Keyboard navigation
* Paste text
* Paste image for OCR
* Drag and drop images for OCR
* Drag and drop Markdown/text files later

Recommended editor candidates:

* TipTap
* Milkdown
* ProseMirror-based custom editor

Default storage should remain Markdown, even if the editor is WYSIWYG-like.

---

# 9. Floating Selection Toolbar

## 9.1 Trigger

When the user selects text inside the editor, show a floating toolbar above or near the selection.

Example:

```txt
[Format] [Ask...] [Read] [Copy]
```

## 9.2 Behavior

### Format

Runs Quick Format on the selected text.

### Ask...

Opens a compact prompt input:

```txt
What should ExoQuill do with this text?
[______________________________________]
[Apply] [Cancel]
```

Examples:

```txt
Mach daraus klare Bullet Points.
Formuliere es etwas professioneller.
Korrigiere nur Rechtschreibung und Zeichensetzung.
Fasse es auf Deutsch zusammen.
Convert this into clean Markdown.
```

### Read

Reads selected text aloud.

### Copy

Copies selected text.

## 9.3 Replacement policy

For selected text:

* Formatting may replace selected text directly.
* A small undo toast must appear.

Toast:

```txt
Formatted selection. [Undo]
```

For whole-note formatting:

* Always show preview/diff before applying.

---

# 10. Dictation Specification

## 10.1 UI

Dictation panel:

```txt
Dictation

Input
[ Microphone Name                      ▼ ]

Gain
[✓] Auto
Input level: ▂▃▅▇▆▃

[Start Dictation]
```

If Auto Gain is off:

```txt
Gain
[ ] Auto
Manual: ━━━━━●────
Input level: ▂▃▅▇▆▃
```

Status states:

```txt
Idle
Listening
Speech detected
Processing
Inserted
Paused
Error
```

## 10.2 Minimal settings

Visible in main dictation UI:

* Input source
* Auto gain toggle
* Manual gain slider only when needed
* Input level meter
* Start/stop button

Hidden in advanced settings:

* VAD sensitivity
* Max session duration
* Max audio buffer length
* Raw audio retention
* Whisper model
* CPU threads
* GPU acceleration
* Language mode

## 10.3 Default dictation mode

Default:

```txt
Language mode: German + English terms
Insert mode: append to active note
Raw audio retention: off
Formatting after dictation: optional
```

## 10.4 Language handling

Supported language modes:

```txt
German + English terms
English
Auto
```

Default:

```txt
German + English terms
```

Behavior:

* Prefer German transcription.
* Preserve English technical terms.
* Use a configurable custom term list.
* Avoid translating English terms to German.
* Avoid changing product/library names.

Initial custom terms may include:

```txt
React
TypeScript
JavaScript
WebGL
WebGPU
API
Frontend
Backend
UI
UX
Commit
Branch
Pull Request
GitHub
Tauri
Rust
Next.js
Markdown
ExoJS
ExoSnap
ExoQuill
```

## 10.5 Audio pipeline

```txt
Microphone input
  ↓
Audio normalization / gain
  ↓
Ring buffer
  ↓
VAD speech segmentation
  ↓
Chunk assembly
  ↓
Speech-to-text provider
  ↓
Raw transcript event
  ↓
Optional formatter
  ↓
Note insertion
```

## 10.6 Segmentation

Recommended defaults:

```txt
Sample rate: 16 kHz
Chunk size: 20–40 ms for VAD
Minimum speech duration: 300 ms
Silence end threshold: 700–1200 ms
Max segment duration: 8–15 seconds
Overlap context: 300–800 ms
```

## 10.7 Dictation history

Every dictation result creates a `note_event`.

Stored:

* Raw transcript
* Processed transcript
* Timestamp
* Note ID
* Provider ID
* Model ID
* Language mode
* Duration
* Confidence metadata if available

Raw audio is not stored by default.

---

# 11. OCR Specification

## 11.1 Input methods

OCR should support:

```txt
File picker
Drag and drop
Ctrl+V clipboard image
Screen region
```

## 11.2 v0.1 OCR sources

Mandatory v0.1:

* File picker
* Drag and drop
* Clipboard image paste

Optional v0.1 / mandatory v0.2:

* Screen region capture

## 11.3 OCR flow

```txt
Image source
  ↓
Image validation
  ↓
Preprocessing
  ↓
OCR provider
  ↓
Raw OCR text
  ↓
Optional formatting
  ↓
Insert into note
```

## 11.4 Preprocessing

Initial preprocessing options:

* Convert to grayscale
* Scale image 2x or 3x
* Increase contrast
* Optional thresholding
* Optional denoise
* Optional deskew later

Default should be automatic and not exposed in the main UI.

## 11.5 OCR language mode

Default:

```txt
deu+eng
```

The user should not need to choose languages in normal use.

## 11.6 OCR insertion behavior

If image is pasted into editor:

```txt
If active note exists:
  OCR image.
  Insert recognized text at cursor.

If no active note exists:
  Create new note.
  Insert recognized text.
```

## 11.7 OCR result handling

After OCR, show a small result toast:

```txt
Text recognized. [Format] [Undo]
```

If OCR confidence is low:

```txt
OCR result may need review. [Open Raw] [Format]
```

---

# 12. Formatting Specification

## 12.1 Purpose

Formatting is not general creative rewriting. It is a controlled cleanup tool.

It should:

* Correct obvious dictation errors
* Correct obvious OCR errors
* Add punctuation
* Improve paragraph structure
* Convert rough text to readable Markdown
* Preserve meaning
* Preserve technical terms
* Avoid hallucinating content

It should not:

* Invent facts
* Add new arguments
* Change intent
* Translate unless explicitly requested
* Remove uncertainty
* Over-polish personal notes unless requested

## 12.2 Modes

### Quick Format

One-click cleanup.

Default behavior:

```txt
Correct spelling, punctuation and obvious recognition mistakes.
Improve readability.
Use clean Markdown where helpful.
Do not add new content.
Preserve meaning and technical terms.
```

### Custom Format

User provides a custom instruction.

Examples:

```txt
Mach daraus Meeting-Notizen mit Bullet Points.
Formuliere es als GitHub-Issue.
Korrigiere nur Rechtschreibung.
Mache daraus eine kurze Zusammenfassung.
Convert this into clean Markdown documentation.
```

## 12.3 Scope

Formatting can apply to:

* Selected text
* Current paragraph
* Whole note

Default UI:

```txt
Selection exists:
  Show floating toolbar.

No selection:
  Toolbar action applies to whole note with preview.
```

## 12.4 Preview policy

Selected text:

* Direct replace allowed.
* Undo toast required.

Whole note:

* Preview/diff required.
* User must confirm apply.

## 12.5 Formatter provider prompt contract

The formatter must receive structured input:

```json
{
  "source": "dictation | ocr | manual | mixed",
  "language_mode": "de+en_terms | en | auto",
  "operation": "quick_format | custom_format",
  "instruction": "optional user instruction",
  "custom_terms": ["React", "TypeScript", "ExoQuill"],
  "text": "raw input text"
}
```

Expected output:

```json
{
  "formatted_text": "...",
  "warnings": [],
  "changed_meaning_risk": "low | medium | high"
}
```

For v0.1, plain text output is acceptable if JSON mode is unreliable, but the internal API should be designed for structured output.

## 12.6 Safety rule

If the model appears to add unsupported new information, ExoQuill should reject the result or show warning:

```txt
This result may have changed the meaning. Review before applying.
```

---

# 13. Read-Aloud / TTS Specification

## 13.1 Purpose

Read-aloud allows users to listen to their notes or selected text using a pleasant local voice.

## 13.2 UI

Minimal read panel:

```txt
Read aloud

Voice
[ German Calm                    ▼ ]

Speed
0.8x ━━━●━━━━ 1.2x

[Read Selection] [Read From Cursor] [Read Note]
```

If text is selected, the floating toolbar shows:

```txt
[Format] [Ask...] [Read]
```

## 13.3 Playback controls

Required controls:

```txt
Play
Pause
Resume
Stop
Skip paragraph forward
Skip paragraph backward
```

Optional later:

```txt
Export audio
Remember read position
Highlight current sentence
```

## 13.4 Read scopes

Supported:

* Selected text
* Current paragraph
* From cursor to end
* Whole note

## 13.5 TTS queue

Read-aloud should process text in segments:

```txt
Note
  ↓
Paragraph splitter
  ↓
Sentence splitter
  ↓
TTS queue
  ↓
Audio playback
```

Reason:

* Better responsiveness
* Easier pause/resume
* Easier skip
* Avoid huge TTS requests
* Lower memory pressure

## 13.6 Voice strategy

v0.1 should support one reliable local TTS provider and one provider interface.

Recommended default strategy:

```txt
Provider interface first.
Bundle or download voices separately.
Do not hardwire the app to one TTS runtime.
```

Initial voice targets:

```txt
German Calm Female
German Calm Male
English Calm Female
English Calm Male
```

If only one high-quality German voice is available at first, ship with one German voice and one English voice.

## 13.7 TTS quality bar

Minimum acceptable read-aloud quality:

* Understandable
* Not robotic like classic system TTS
* Stable pronunciation
* Handles punctuation
* Handles German umlauts
* Does not crash on Markdown
* Skips code fences or reads them in a controlled way

## 13.8 Markdown read behavior

Default:

* Headings are read as headings.
* Bullet lists are read naturally.
* Links read only visible text.
* Code blocks are skipped or read with explicit setting.
* Tables are simplified.
* Frontmatter is skipped.

---

# 14. Model and Provider Specification

## 14.1 Provider architecture

All AI features use provider interfaces.

```txt
SpeechToTextProvider
OcrProvider
FormatterProvider
TextToSpeechProvider
VadProvider
```

Each provider must expose:

```txt
id
display_name
version
capabilities
required_models
license_info
health_check()
run()
cancel()
```

## 14.2 Speech-to-text providers

### v0.1 default

```txt
whisper.cpp
```

### Later optional

```txt
faster-whisper
Vosk
OS-native speech APIs as optional non-default provider
```

## 14.3 VAD providers

### v0.1 default

```txt
Silero VAD ONNX
```

### Fallback

```txt
WebRTC VAD or simple energy threshold
```

## 14.4 OCR providers

### v0.1 default

```txt
Tesseract
Languages: deu + eng
```

### Later optional

```txt
PaddleOCR
EasyOCR
OS-native OCR provider
```

## 14.5 Formatter providers

### v0.1 default

```txt
llama.cpp provider
Qwen3 small/medium instruct model
```

Suggested model profiles:

```txt
Fast:
  Qwen3 1.7B/4B quantized

Balanced:
  Qwen3 4B/8B quantized

Quality:
  larger Qwen3 model if user has enough RAM/VRAM
```

## 14.6 TTS providers

### v0.1 target

```txt
Provider-based local TTS runtime
```

Candidate providers:

```txt
Piper:
  Practical local TTS with German/English voices.
  Watch licensing carefully depending on runtime/voice source.

Kokoro:
  Lightweight and pleasant for supported languages.
  German support should be treated as experimental until verified.

System TTS:
  Optional fallback.
  Not privacy-risky, but voice quality varies and may not meet product bar.
```

## 14.7 Model manager

ExoQuill should not ship all large models in the installer by default.

Model Manager must support:

* List available models
* Download model
* Verify hash
* Show license
* Show size
* Delete model
* Select default model per feature
* Show installed/missing state

## 14.8 Model manifest

Example:

```json
{
  "models": [
    {
      "id": "stt.whisper.small.de-en.q5",
      "feature": "speech_to_text",
      "provider": "whisper.cpp",
      "display_name": "Whisper Small Balanced",
      "languages": ["de", "en"],
      "size_mb": 500,
      "license": "MIT / model license",
      "download_url": "...",
      "sha256": "...",
      "recommended": true
    }
  ]
}
```

---

# 15. Settings Specification

## 15.1 Main settings sections

```txt
General
Notes
Dictation
OCR
Formatting
Read Aloud
Models
Privacy
Shortcuts
Advanced
```

## 15.2 General

* Theme: system / light / dark
* Start minimized: on/off
* Show tray icon: on/off
* Launch at startup: on/off
* Default note behavior

## 15.3 Notes

* Default insert mode
* Auto-title notes
* Archive instead of delete
* Search indexing
* Export location

## 15.4 Dictation

Visible simple settings:

* Default microphone
* Auto gain default
* Manual gain default
* Default language mode

Advanced:

* VAD sensitivity
* Silence timeout
* Max segment duration
* Max session duration
* Raw audio retention
* STT model
* CPU threads
* GPU acceleration

## 15.5 OCR

* OCR provider
* Default language mode: German + English
* Auto-format OCR results: off/on
* Preprocessing level: conservative / balanced / aggressive

## 15.6 Formatting

* Formatter model
* Quick Format behavior
* Whole-note preview required: always on
* Custom terms
* Default style:

  * Clean prose
  * Markdown notes
  * Meeting notes
  * Developer notes

## 15.7 Read Aloud

* TTS provider
* Default voice
* Default speed
* Read code blocks: off/on
* Highlight current sentence: later
* Remember read position: later

## 15.8 Privacy

* Store raw transcripts: on/off
* Store raw OCR text: on/off
* Store raw audio: never/session/manual
* Delete captures older than N days
* Clear model cache
* Clear note event history
* Export all data
* Delete all local data

## 15.9 Shortcuts

Default suggestions:

```txt
Toggle Dictation: Ctrl+Alt+D
OCR Screen Region: Ctrl+Alt+O
Quick Note: Ctrl+Alt+N
Read Selection: Ctrl+Alt+R
Quick Format Selection: Ctrl+Alt+F
```

Shortcuts must be configurable.

---

# 16. Data Model

## 16.1 Database

Use SQLite.

## 16.2 Tables

### notes

```sql
CREATE TABLE notes (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  content_markdown TEXT NOT NULL DEFAULT '',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  pinned INTEGER NOT NULL DEFAULT 0,
  archived INTEGER NOT NULL DEFAULT 0,
  deleted_at TEXT,
  language_mode TEXT NOT NULL DEFAULT 'de_en_terms',
  last_cursor_position INTEGER DEFAULT 0
);
```

### note_events

```sql
CREATE TABLE note_events (
  id TEXT PRIMARY KEY,
  note_id TEXT NOT NULL,
  source_type TEXT NOT NULL,
  raw_text TEXT,
  processed_text TEXT,
  operation TEXT,
  provider_id TEXT,
  model_id TEXT,
  model_version TEXT,
  confidence_json TEXT,
  metadata_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (note_id) REFERENCES notes(id)
);
```

Source types:

```txt
manual
dictation
ocr_file
ocr_drag_drop
ocr_clipboard
ocr_region
format_selection
format_note
tts_read
audio_file
import
```

### captures

```sql
CREATE TABLE captures (
  id TEXT PRIMARY KEY,
  note_event_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  local_path TEXT,
  mime_type TEXT,
  size_bytes INTEGER,
  retained_until TEXT,
  metadata_json TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (note_event_id) REFERENCES note_events(id)
);
```

Capture kinds:

```txt
audio_chunk
audio_file
image_file
clipboard_image
screenshot_region
```

### settings

```sql
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

### models

```sql
CREATE TABLE models (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL,
  feature TEXT NOT NULL,
  display_name TEXT NOT NULL,
  local_path TEXT,
  installed INTEGER NOT NULL DEFAULT 0,
  sha256 TEXT,
  size_bytes INTEGER,
  license TEXT,
  metadata_json TEXT,
  installed_at TEXT
);
```

### jobs

```sql
CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  status TEXT NOT NULL,
  note_id TEXT,
  input_json TEXT,
  result_json TEXT,
  error TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT
);
```

Job statuses:

```txt
queued
running
completed
failed
cancelled
```

---

# 17. Internal Command/API Specification

## 17.1 Frontend commands

The frontend should call backend commands rather than directly controlling model processes.

Examples:

```ts
createNote(input)
updateNote(input)
deleteNote(noteId)
listNotes()
searchNotes(query)

startDictation(options)
stopDictation()
setDictationTarget(noteId)

runOcrFromFile(filePath, options)
runOcrFromClipboard(options)
runOcrFromRegion(region, options)

formatSelection(noteId, range, instruction?)
formatNote(noteId, instruction?)

readSelection(text, options)
readNote(noteId, options)
pauseReadAloud()
resumeReadAloud()
stopReadAloud()

listModels()
installModel(modelId)
deleteModel(modelId)
setDefaultModel(feature, modelId)
```

## 17.2 Event bus

Backend emits events:

```txt
dictation:state_changed
dictation:level
dictation:partial_text
dictation:final_text
ocr:started
ocr:completed
format:started
format:completed
tts:started
tts:progress
tts:completed
job:updated
models:changed
notes:changed
error
```

## 17.3 Job queue

All AI tasks must run as jobs.

Rules:

* UI never blocks on AI inference.
* Jobs can be cancelled.
* Long-running jobs report progress.
* Only one heavy model job runs by default.
* TTS and STT should not starve UI updates.
* Background jobs must respect resource limits.

---

# 18. Privacy and Security

## 18.1 Privacy defaults

Default behavior:

```txt
No cloud calls.
No telemetry.
No raw audio retention.
No automatic upload.
No automatic model download without user action.
```

## 18.2 Local data

Stored locally:

* Notes
* Settings
* Model metadata
* Optional note events
* Optional captures

Not stored by default:

* Raw audio
* Full screenshot images after OCR
* Temporary preprocessed OCR images

## 18.3 First-run onboarding

First-run screen must explain:

```txt
ExoQuill runs locally.
Core features need local models.
You choose which models to download.
Raw audio is not stored by default.
You can delete all local data anytime.
```

## 18.4 License transparency

Model Manager must show:

* Runtime license
* Model license
* Voice license
* File size
* Download source
* Hash

## 18.5 Dangerous action confirmations

Require confirmation for:

* Delete note
* Delete all notes
* Clear all data
* Enable raw audio retention
* Enable external provider
* Install large model

---

# 19. Packaging and Distribution

## 19.1 Installer strategy

v0.1:

```txt
Windows installer
Windows portable build
```

Later:

```txt
Linux AppImage/deb/rpm
macOS dmg
```

## 19.2 Model packaging

Do not bundle large models in the default installer.

Instead:

```txt
App installer:
  app binary
  provider runtime binaries where license-compatible
  no large models

First-run:
  model manager downloads selected models
```

Optional GitHub release assets later:

```txt
exoquill-vX.Y.Z-windows-x64-setup.exe
exoquill-vX.Y.Z-windows-x64-portable.zip
exoquill-vX.Y.Z-model-manifest.json
exoquill-vX.Y.Z-full-offline-pack.zip
```

## 19.3 Offline pack

Later optional:

```txt
exoquill-vX.Y.Z-full-offline-pack.zip
  app/
  models/
    stt/
    ocr/
    formatter/
    tts/
  licenses/
  manifest.json
```

---

# 20. Repository Structure

Recommended monorepo:

```txt
exoquill/
  README.md
  LICENSE
  CHANGELOG.md
  CONTRIBUTING.md
  SECURITY.md
  package.json
  pnpm-workspace.yaml

  apps/
    desktop/
      src/
        app/
        components/
        editor/
        notes/
        dictation/
        ocr/
        formatting/
        read-aloud/
        settings/
      src-tauri/
        src/
        tauri.conf.json
        Cargo.toml

  crates/
    exoquill-core/
      src/
        notes/
        settings/
        jobs/
        events/
    exoquill-ai/
      src/
        providers/
        stt/
        ocr/
        formatter/
        tts/
        vad/
    exoquill-audio/
      src/
    exoquill-capture/
      src/
    exoquill-db/
      src/

  .workspace/
    specs/
      exoquill-product-spec.md
    reports/
    prompts/
    scratch/

  models/
    manifest.schema.json
    manifest.dev.json

  docs/
    architecture.md
    privacy.md
    model-management.md
    provider-api.md
    ux.md

  scripts/
    verify-model-manifest.ts
    package-offline-pack.ts
```

Internal planning artifacts should stay under `.workspace/`.

---

# 21. Testing Strategy

## 21.1 Unit tests

Required areas:

* Note creation
* Note sorting
* Auto-create note resolver
* Insert at cursor
* Replace selection
* Job queue state transitions
* Settings serialization
* Model manifest validation
* Formatter prompt construction
* OCR input validation
* TTS text segmentation

## 21.2 Integration tests

* Create note → OCR image → insert text
* Create note → dictation mock result → append text
* Selection → quick format mock → replace selection
* Whole note → format preview → apply
* Selection → TTS mock → playback queue
* Clipboard image mock → OCR job
* Missing model → model install prompt

## 21.3 Golden tests

Use fixtures:

```txt
German dictation rough text
German + English technical terms
English dictation rough text
OCR UI screenshot text
OCR document text
Markdown formatting examples
TTS paragraph segmentation examples
```

## 21.4 Manual QA checklist

Before release:

* Install fresh app
* First-run onboarding
* Download one STT model
* Download one OCR language pack
* Download one formatter model
* Download one TTS voice
* Create note
* Dictate German text
* Dictate German text with English terms
* OCR pasted image
* OCR dragged image
* Format selected text
* Format whole note with preview
* Read selected text
* Read whole note
* Restart app
* Verify notes persist
* Delete all local data

---

# 22. Accessibility

Minimum requirements:

* Keyboard navigable main UI
* Visible focus states
* Screen-reader-friendly labels
* Sufficient contrast
* Tooltips for icon buttons
* No color-only status indicators
* Configurable font size
* TTS controls reachable by keyboard

---

# 23. Performance Targets

## 23.1 App

* Cold start under 3 seconds without model load
* UI remains responsive during model inference
* Note editor handles large notes up to at least 100k characters
* Search over local notes should feel instant for typical usage

## 23.2 Dictation

* Audio level meter latency under 100 ms
* Speech segment finalization usually within 1–3 seconds after silence, depending on model/hardware
* No unbounded memory growth during long sessions
* Session limit enforced

## 23.3 OCR

* Small screenshot OCR should complete quickly on CPU
* Large images should show progress or busy state
* Preprocessing should not freeze UI

## 23.4 Formatting

* Selection formatting should stream or show progress
* Whole-note formatting should use preview
* Long notes may require chunking

## 23.5 TTS

* Playback should begin after first generated segment, not after full note generation
* Pause/stop should be immediate
* Long notes should be read as a queue

---

# 24. Error Handling

## 24.1 Missing model

```txt
This feature needs a local model.
[Install recommended model] [Choose model]
```

## 24.2 Dictation no microphone

```txt
No microphone detected.
Check your input device or system permissions.
```

## 24.3 OCR no text found

```txt
No readable text found.
Try a clearer image or crop the text area.
```

## 24.4 Formatter failed

```txt
Formatting failed.
Your original text was not changed.
```

## 24.5 TTS failed

```txt
Read-aloud failed.
Try another voice or restart the local TTS provider.
```

## 24.6 Provider crashed

```txt
The local AI provider stopped unexpectedly.
[Restart provider] [View details]
```

---

# 25. Open Source Project Requirements

## 25.1 License

Recommended app license:

```txt
Apache-2.0 or MIT
```

Decision required before bundling GPL components.

If GPL TTS runtime is bundled or linked directly, reassess app license compatibility.

## 25.2 README structure

README should include:

* What ExoQuill is
* What it is not
* Feature list
* Privacy statement
* Supported languages
* Installation
* Model setup
* Development setup
* Roadmap
* Contribution rules
* License notes

## 25.3 Contribution policy

Require:

* Focused PRs
* Tests for core changes
* No telemetry by default
* No cloud dependency for core features
* Provider additions must document license/model size/source
* Commit completed changes

## 25.4 Issue labels

Suggested labels:

```txt
area:notes
area:dictation
area:ocr
area:formatting
area:tts
area:models
area:ui
area:desktop
area:privacy
area:packaging
type:bug
type:feature
type:tech-debt
type:research
good-first-issue
blocked
```

---

# 26. Implementation Plan

## PR 1 — App Skeleton + Notes Core

Goal:

* Create the Tauri desktop app.
* Implement local notes without AI.

Included:

* Tauri v2 app shell
* React/TypeScript frontend
* SQLite database setup
* Notes table and migrations
* Notes sidebar
* Markdown editor
* Create/update/delete note
* Sort by updated_at
* Auto-create-note resolver
* Basic settings table
* Tests

Not included:

* AI models
* OCR
* STT
* TTS
* Formatting model

Acceptance criteria:

* User can create, edit and delete notes.
* Notes persist after restart.
* Notes sort by last modified date.
* If no note is selected, resolver can create a note programmatically.
* All completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 2 — Provider Interfaces + Job Queue

Goal:

* Add the internal architecture for AI jobs without real AI providers.

Included:

* Provider traits/interfaces
* Job queue
* Event bus
* Mock STT provider
* Mock OCR provider
* Mock formatter provider
* Mock TTS provider
* UI job status handling
* Tests

Acceptance criteria:

* Mock jobs run asynchronously.
* UI does not block.
* Job status persists or is observable.
* Failed jobs show errors.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 3 — OCR v0.1

Goal:

* Add real image-to-text capture.

Included:

* File picker OCR
* Drag and drop OCR
* Ctrl+V image OCR
* Tesseract provider
* deu+eng language support
* Basic preprocessing
* Insert result into active note
* Auto-create note if needed
* OCR note event history
* Tests with fixture images

Acceptance criteria:

* OCR from file works.
* OCR from clipboard works.
* OCR result inserts into note.
* No selected note creates a new note.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 4 — Formatting v0.1

Goal:

* Add local text cleanup.

Included:

* Quick Format selected text
* Custom instruction selected text
* Whole-note format preview
* Formatter provider via llama.cpp-compatible interface
* Prompt templates
* Custom terms
* Undo toast
* Tests with mock provider and prompt snapshots

Acceptance criteria:

* Selection can be formatted and replaced.
* Whole note uses preview before apply.
* Formatter preserves technical terms.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 5 — Dictation v0.1

Goal:

* Add minimal local dictation.

Included:

* Microphone selection
* Auto gain toggle
* Manual gain slider
* Audio level meter
* Start/stop dictation
* Whisper provider
* German + English terms default
* Append to active note
* Auto-create note if needed
* Raw audio not stored by default
* Dictation note events
* Tests where possible

Acceptance criteria:

* User can dictate into active note.
* No active note creates a new note.
* UI stays responsive.
* Audio level meter works.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 6 — Read Aloud v0.1

Goal:

* Add local TTS playback.

Included:

* Read selected text
* Read whole note
* Read from cursor
* Voice selector
* Speed control
* Play/pause/stop
* Paragraph queue
* TTS provider interface
* Initial local TTS provider
* Tests for text segmentation and queue behavior

Acceptance criteria:

* Selected text can be read aloud.
* Whole note can be read aloud.
* Playback can pause and stop.
* Long notes are segmented.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

## PR 7 — Tray + Shortcuts

Goal:

* Make ExoQuill useful in background.

Included:

* System tray
* Global shortcut registration
* Toggle dictation shortcut
* OCR shortcut placeholder or implementation
* Quick note shortcut
* Settings UI for shortcuts
* Autostart option
* Tests where possible

Acceptance criteria:

* App can run in tray.
* User can trigger dictation globally.
* Shortcuts are configurable.
* Completed changes are committed.

Recommended agent:

```txt
Claude Opus high
```

---

# 27. Default Product Decisions

These are the current recommended defaults:

```txt
App name: ExoQuill
Architecture: Tauri v2 + React + TypeScript + Rust
Database: SQLite
Editor: Markdown-first WYSIWYG-like editor
STT: whisper.cpp provider
VAD: Silero VAD provider
OCR: Tesseract provider with deu+eng
Formatter: llama.cpp provider with Qwen3 instruct model
TTS: provider-based local runtime; final runtime pending license/quality verification
Default language mode: German + English terms
Default dictation insert mode: append to active note
Default raw audio retention: off
Default OCR format after extraction: off, with visible Format action
Default whole-note formatting: preview required
Default selection formatting: direct replace with undo
```

---

# 28. Open Questions

These decisions remain open:

1. App license:

   * MIT?
   * Apache-2.0?
   * GPL-compatible if bundling GPL TTS runtime?

2. Initial TTS provider:

   * Piper?
   * Kokoro?
   * System TTS fallback?
   * Multiple provider support from day one?

3. Editor:

   * TipTap?
   * Milkdown?
   * Custom ProseMirror integration?

4. Screen region OCR:

   * v0.1 must-have?
   * v0.2 milestone?

5. Model manager:

   * custom download UI in v0.1?
   * manual model path selection first?

6. Formatting UX:

   * direct replacement for selected text?
   * always preview?
   * configurable?

7. External insertion:

   * should ExoQuill eventually type into the currently focused app?
   * or remain note-internal only?

---

# 29. Recommended v0.1 Definition of Done

ExoQuill v0.1 is done when:

* The app installs and starts on Windows.
* Notes work locally and persist.
* User can dictate into a note.
* User can OCR an image file.
* User can OCR a pasted clipboard image.
* User can quick-format selected text.
* User can custom-format selected text.
* User can preview whole-note formatting.
* User can read selected text aloud.
* User can read whole note aloud.
* No cloud account is required.
* No telemetry exists.
* Raw audio is not stored by default.
* Model licenses are visible.
* The README clearly explains local model setup.
* The project has tests for notes, jobs, formatting, OCR input handling and TTS segmentation.
