// Lightweight bilingual (German / English) UI localization.
//
// No provider is needed: the active language lives in a module-level store that
// components subscribe to via `useI18n()` (a `useSyncExternalStore` hook), so a
// language switch re-renders every consumer. The choice is persisted in
// localStorage and defaults to the browser language (German-first).

import { useSyncExternalStore } from "react";

export type Lang = "de" | "en";

const STORAGE_KEY = "exoquill-lang";

// German strings are the source of truth; `TranslationKey` is derived from them,
// and the English table is type-checked to cover exactly the same keys.
const de = {
  // -- shared --
  "common.close": "Schließen",
  "common.cancel": "Abbrechen",
  "common.apply": "Übernehmen",
  "common.delete": "Löschen",
  "common.reset": "Zurücksetzen",

  // -- top toolbar (global controls) --
  "toolbar.models": "Modelle",
  "toolbar.models.title": "On-Device-Modelle & Lizenzen",
  "toolbar.theme.title": "Theme wechseln",
  "toolbar.theme.aria": "Hell-/Dunkel-Theme umschalten",
  "toolbar.lang.title": "Sprache / Language",
  "toolbar.lang.aria": "Sprache wechseln",

  // -- action bar --
  "action.dictate": "Diktieren",
  "action.stop": "Stopp",
  "action.dictate.title": "In diese Notiz diktieren",
  "action.dictate.stopTitle": "Diktat stoppen",
  "action.ocr": "OCR",
  "action.ocr.title": "Ein Bild per Texterkennung in diese Notiz einlesen",
  "action.format": "Formatieren",
  "action.formatting": "Formatiere…",
  "action.format.title": "Auswahl – oder die gesamte Notiz – formatieren",
  "action.read": "Vorlesen",
  "action.read.title": "Auswahl oder Notiz vorlesen",
  "action.export": "Export",
  "action.export.title": "Diese Notiz als Markdown exportieren",
  "action.history": "Verlauf",
  "action.history.title": "Verlauf dieser Notiz anzeigen",
  "action.delete.title": "Notiz löschen",

  // -- sidebar --
  "sidebar.search": "Notizen durchsuchen",
  "sidebar.count": "{count} NOTIZEN",
  "sidebar.archivedCount": "{count} ARCHIVIERT",
  "sidebar.trashCount": "{count} ELEMENTE",
  "sidebar.newNote": "Neue Notiz",
  "note.emptyPreview": "Leere Notiz",

  // -- sidebar scopes + sort + groups --
  "scope.active": "Aktiv",
  "scope.archived": "Archiviert",
  "scope.trash": "Papierkorb",
  "scope.archivedEmpty": "Keine archivierten Notizen.",
  "scope.trashEmpty": "Der Papierkorb ist leer.",
  "scope.activeEmpty": "Noch keine Notizen.",
  "sort.aria": "Sortierung",
  "sort.modified": "Zuletzt geändert",
  "sort.created": "Erstellt",
  "sort.title": "Titel",
  "group.pinned": "ANGEHEFTET",
  "group.allNotes": "ALLE NOTIZEN",

  // -- note actions (context menu, hover, bulk bar) --
  "noteAction.menu": "Aktionen",
  "noteAction.pin": "Anheften",
  "noteAction.unpin": "Loslösen",
  "noteAction.rename": "Umbenennen",
  "noteAction.duplicate": "Duplizieren",
  "noteAction.archive": "Archivieren",
  "noteAction.export": "Exportieren",
  "noteAction.toTrash": "In den Papierkorb",
  "noteAction.restore": "Wiederherstellen",
  "noteAction.deleteForever": "Endgültig löschen",
  "trash.emptyTrash": "Papierkorb leeren",
  "trash.deletedAgo": "Gelöscht {when}",
  "trash.daysLeft": "noch {count} Tage",
  "trash.retention": "Papierkorb-Einträge werden nach 30 Tagen entfernt.",
  "archive.archivedAgo": "Archiviert {when}",

  // -- multi-select bulk bar --
  "select.count": "{count} ausgewählt",
  "select.cancel": "Abbrechen",

  // -- undo toasts --
  "toast.undo": "Rückgängig",
  "toast.trashed": "Notiz in den Papierkorb verschoben",
  "toast.trashedMany": "{count} Notizen in den Papierkorb verschoben",
  "toast.archived": "Notiz archiviert",
  "toast.archivedMany": "{count} Notizen archiviert",
  "toast.restored": "Notiz wiederhergestellt",
  "toast.deletedForever": "Notiz endgültig gelöscht",
  "toast.versionRestored": "Version wiederhergestellt",

  // -- relative time --
  "time.justNow": "gerade eben",
  "time.minutesAgo": "vor {count} Min",
  "time.hoursAgo": "vor {count} Std",
  "time.yesterday": "gestern",
  "time.daysAgo": "vor {count} Tagen",

  // -- editor + meta --
  "editor.placeholder": "Schreib los oder erfasse etwas…",
  "editor.titlePlaceholder": "Unbenannte Notiz",
  "meta.draft": "ENTWURF",
  "meta.empty": "LEER",
  "meta.words": "{count} WÖRTER",

  // -- empty state --
  "empty.title": "Keine Notiz ausgewählt",
  "empty.desc": "Beginne mit einer Notiz, diktiere etwas oder füge einen Screenshot ein.",

  // -- dictation --
  "dictation.source": "Diktat-Quelle",
  "dictation.defaultMic": "Standard-Mikrofon",
  "dictation.recording": "Aufnahme läuft…",

  // -- read-aloud toolbar --
  "readaloud.label": "Vorlesen",
  "readaloud.backend.aria": "Vorlese-Backend",
  "readaloud.backend.title": "Sprachmodell / Backend",
  "readaloud.voice.aria": "Vorlese-Stimme",
  "readaloud.emotion.aria": "Vorlese-Stimmung",
  "readaloud.emotion.title": "Stimmung / Emotion (Zonos)",
  "readaloud.exportAudio.aria": "Audio exportieren",
  "readaloud.exportAudio.title": "Vorgelesenes Audio als WAV speichern",
  "readaloud.settings.aria": "Weitere Vorlese-Einstellungen",

  // -- read-aloud status bars --
  "readaloud.prepAudio": "Bereite Audio vor … {done}/{total}",
  "readaloud.prepText": "Bereite Text vor … {done}/{total}",
  "readaloud.preparing": "Wird vorbereitet …",
  "readaloud.paused": "Pausiert",
  "readaloud.reading": "Liest vor…",
  "readaloud.prepSuffix": " · bereitet {done}/{total} auf",
  "readaloud.resume": "▶ Fortsetzen",
  "readaloud.pause": "⏸ Pause",
  "readaloud.stop": "⏹ Stopp",
  "readaloud.cancel": "✕ Abbrechen",
  "readaloud.voiceLoading":
    "Stimme lädt noch … das Vorlesen startet automatisch, sobald sie bereit ist.",
  "readaloud.sample":
    "Dies ist eine kurze Hörprobe der aktuellen Stimme und der gewählten Einstellungen.",
  "readaloud.exportName": "vorlesen",

  // -- formatting --
  "format.progress": "Formatiere … {done}/{total}",

  // -- OCR --
  "ocr.busy": "Texterkennung läuft…",
  "ocr.failed": "Texterkennung fehlgeschlagen: {error}",
  "ocr.regionFailed": "Bereichs-OCR fehlgeschlagen: {error}",
  "ocr.title": "Texterkennung",
  "ocr.insert": "In Notiz",
  "ocr.insert.title": "Auswahl – oder den gesamten Text – in die Notiz übernehmen",
  "ocr.copy": "Kopieren",
  "ocr.copy.title": "Auswahl – oder den gesamten Text – kopieren (Strg+C)",
  "ocr.selectAll": "Alles auswählen",
  "ocr.selectAll.title": "Den gesamten erkannten Text markieren (Strg+A)",
  "ocr.hint":
    "Text markieren und kopieren oder „In Notiz“ übernehmen — ohne Auswahl wird der gesamte erkannte Text genutzt.",
  "ocr.imageAlt": "Erkanntes Bild",

  // -- edit-history (diff) overlay --
  "history.title": "Verlauf",
  "history.empty": "Noch keine Ereignisse.",
  "history.changesOnly": "NUR ÄNDERUNGEN",
  "history.versions": "{count} VERSIONEN",
  "history.current": "Aktueller Stand",
  "history.words": "Wörter",
  "history.compare": "VERGLEICH",
  "history.thisVersion": "Diese Version",
  "history.currentState": "Aktuell",
  "history.readOnly": "NUR-LESEN",
  "history.restoreVersion": "Diese Version wiederherstellen",
  "history.restoreHint": "Schreibt als neue, undobare Änderung – nicht destruktiv",
  "history.selectVersion": "Wähle links eine Version, um den Diff zu sehen.",
  "history.op.format": "FORMATIERT",
  "history.op.manual": "MANUELL",
  "history.op.ocr": "OCR",
  "history.op.dictation": "DIKTAT",
  "history.op.restore": "WIEDERHERGESTELLT",
  "history.op.snapshot": "SNAPSHOT",

  // -- formatting preview --
  "preview.title": "Formatierung – Vorschau",
  "preview.aria": "Formatierungs-Vorschau",
  "preview.original": "Original",
  "preview.formatted": "Formatiert",

  // -- status bar --
  "status.saved": "GESPEICHERT",
  "status.saving": "SPEICHERT…",
  "status.words": "{count} WÖRTER",
  "status.readingTime": "{count} MIN LESEZEIT",

  // -- model manager --
  "models.title": "Modelle verwalten",
  "models.tier.bundled": "gebündelt",
  "models.tier.download": "Download",
  "models.tier.gated": "Lizenz nötig",
  "models.nonCommercial": "nicht-kommerziell",
  "models.loading": "lädt…",
  "models.setup": "Setup:",
  "models.installed": "installiert ✓",
  "models.install": "Installieren",
  "models.activeProviders": "Aktive Provider",
  "models.confirmNonCommercial":
    "„{name}“ steht unter {license} (nicht für kommerzielle Nutzung). Trotzdem installieren?",
  "models.confirmDelete": "„{name}“ löschen?",
  "models.installFailed": "Installation fehlgeschlagen: {error}",

  // -- read-aloud settings dialog --
  "raSettings.title": "Vorlese-Einstellungen · {backend}",
  "raSettings.aria": "Vorlese-Einstellungen",
  "raSettings.onlySpeed":
    "{backend} nutzt nur das Tempo; weitere Klangregler gibt es nur für Piper und Zonos.",
  "raSettings.speechPrep": "Für Sprache aufbereiten (LLM)",
  "raSettings.speechPrepHint":
    "Schreibt Text vor dem Vorlesen in flüssige Sätze um — bessere Qualität, aber Vorlauf.",
  "raSettings.previewing": "▶ Probe läuft …",
  "raSettings.preview": "▶ Probe abspielen",
  "slider.speed": "Tempo",
  "slider.speed.hint": "Sprechgeschwindigkeit",
  "slider.expressiveness": "Ausdruck",
  "slider.expressiveness.hint": "Klangvariation (noise_scale)",
  "slider.cadence": "Rhythmus",
  "slider.cadence.hint": "Längenvariation (noise_w)",
  "slider.sentenceSilence": "Satzpause",
  "slider.sentenceSilence.hint": "Sekunden Stille nach jedem Satz",
  "slider.intonation": "Intonation",
  "slider.intonation.hint": "Lebhaftigkeit der Betonung (monoton ↔ lebhaft)",
  "slider.brightness": "Klangfarbe",
  "slider.brightness.hint": "Höhenanteil (wärmer ↔ brillanter)",

  // -- Zonos emotion presets --
  "emotion.neutral": "Neutral",
  "emotion.happy": "Fröhlich",
  "emotion.lively": "Lebhaft",
  "emotion.surprised": "Überrascht",
  "emotion.calm": "Ruhig",
  "emotion.sad": "Traurig",
  "emotion.fearful": "Ängstlich",
  "emotion.angry": "Wütend",
  "emotion.disgust": "Angewidert",

  // -- settings window (Bereich 3) --
  "settings.title": "Einstellungen",
  "settings.tab.models": "Modelle",
  "settings.tab.readaloud": "Vorlesen",
  "settings.tab.dictation": "Diktat",
  "settings.tab.appearance": "Darstellung",
  "settings.tab.about": "Über",
  // Models tab
  "settings.models.catalogTitle": "Modellkatalog",
  "settings.models.catalogHint":
    "Modelle laufen vollständig on-device. „Lizenz nötig“ bedeutet nicht-kommerziell oder eine Zustimmung vor dem Download.",
  "settings.models.group.voices": "Stimmen (TTS)",
  "settings.models.group.runtimes": "Multilinguale Runtimes",
  "settings.models.group.other": "Weitere Modelle",
  "settings.models.providersTitle": "Aktive Provider & Status",
  "settings.models.empty": "Keine Modelle im Katalog.",
  "settings.models.size": "Größe",
  "settings.models.setupHint": "Per Setup-Skript installieren:",
  // Read-aloud tab
  "settings.readaloud.voiceTitle": "Stimme & Backend",
  "settings.readaloud.tuningTitle": "Klang & Tempo",
  "settings.readaloud.prepTitle": "Sprachaufbereitung",
  "settings.readaloud.intro":
    "Diese Regler gelten für das aktive Backend ({backend}). Stimme und Backend wählst du in der Lese-Leiste.",
  // Dictation tab
  "settings.dictation.sourceTitle": "Aufnahmequelle",
  "settings.dictation.source": "Quelle",
  "settings.dictation.loopback": "System-Audio (Loopback)",
  "settings.dictation.languageTitle": "Sprache",
  "settings.dictation.language": "Diktatsprache",
  "settings.dictation.langAuto": "Automatisch",
  "settings.dictation.langDe": "Deutsch",
  "settings.dictation.langEn": "Englisch",
  "settings.dictation.vadTitle": "Erkennung & Pegel",
  "settings.dictation.useSilero": "Silero-VAD",
  "settings.dictation.useSileroHint": "Präzisere Sprachpausen-Erkennung (etwas mehr Rechenlast).",
  "settings.dictation.autoGain": "Automatische Aussteuerung",
  "settings.dictation.autoGainHint": "Passt den Pegel automatisch an die Lautstärke an.",
  "settings.dictation.gain": "Mikrofon-Verstärkung",
  // Appearance tab
  "settings.appearance.themeTitle": "Theme",
  "settings.appearance.themeLight": "Hell",
  "settings.appearance.themeDark": "Dunkel",
  "settings.appearance.themeSystem": "System",
  "settings.appearance.languageTitle": "Sprache",
  "settings.appearance.editorTitle": "Editor",
  "settings.appearance.fontSize": "Schriftgröße",
  "settings.appearance.contentWidth": "Inhaltsbreite",
  // About tab
  "settings.about.tagline": "Datenschutzfreundliche, vollständig on-device Notiz-App.",
  "settings.about.version": "Version",
  "settings.about.licenseTitle": "Lizenz",
  "settings.about.licenseBody":
    "ExoQuill steht unter der GPL-3.0. Modell-Gewichte werden zur Laufzeit geladen und tragen ihre eigenen Lizenzen.",
  "settings.about.componentsTitle": "Verwendete On-Device-Komponenten",
  "settings.about.creditsTitle": "Open-Source-Credits",
  "settings.about.creditsBody":
    "Whisper.cpp · llama.cpp · Tesseract · Piper · Tiptap · Tauri — Dank an alle Projekte.",
  "settings.feature.stt": "Spracherkennung",
  "settings.feature.ocr": "Texterkennung",
  "settings.feature.formatter": "Formatierung",
  "settings.feature.tts": "Sprachausgabe",

  // -- command palette + shortcuts (Bereich 4) --
  "palette.placeholder": "Befehl suchen …",
  "palette.empty": "Keine Treffer.",
  "palette.group.actions": "Aktionen",
  "palette.group.view": "Ansicht & Einstellungen",
  "palette.cmd.settings": "Einstellungen öffnen",
  "palette.cmd.shortcuts": "Tastenkürzel anzeigen",
  "palette.cmd.theme": "Theme umschalten",
  "palette.cmd.lang": "Sprache umschalten",
  "palette.cmd.palette": "Befehlspalette",
  "palette.cmd.import": "Datei importieren",
  "shortcuts.title": "Tastenkürzel",
  "shortcuts.group.general": "Allgemein",
  "shortcuts.group.actions": "Notiz-Aktionen",
  "shortcuts.newNote": "Neue Notiz",
  "selection.newNote": "Als neue Notiz",
  "selection.exportName": "Auswahl",

  // -- region OCR overlay --
  "region.hint": "Bereich aufziehen · Esc bricht ab",
} as const;

export type TranslationKey = keyof typeof de;

const en: Record<TranslationKey, string> = {
  "common.close": "Close",
  "common.cancel": "Cancel",
  "common.apply": "Apply",
  "common.delete": "Delete",
  "common.reset": "Reset",

  "toolbar.models": "Models",
  "toolbar.models.title": "On-device models & licenses",
  "toolbar.theme.title": "Toggle theme",
  "toolbar.theme.aria": "Toggle light/dark theme",
  "toolbar.lang.title": "Sprache / Language",
  "toolbar.lang.aria": "Switch language",

  "action.dictate": "Dictate",
  "action.stop": "Stop",
  "action.dictate.title": "Dictate into this note",
  "action.dictate.stopTitle": "Stop dictation",
  "action.ocr": "OCR",
  "action.ocr.title": "OCR an image into this note",
  "action.format": "Format",
  "action.formatting": "Formatting…",
  "action.format.title": "Format the selection, or the whole note",
  "action.read": "Read",
  "action.read.title": "Read the selection or note aloud",
  "action.export": "Export",
  "action.export.title": "Export this note as Markdown",
  "action.history": "History",
  "action.history.title": "Show this note's event history",
  "action.delete.title": "Delete note",

  "sidebar.search": "Search notes",
  "sidebar.count": "{count} NOTES",
  "sidebar.archivedCount": "{count} ARCHIVED",
  "sidebar.trashCount": "{count} ITEMS",
  "sidebar.newNote": "New note",
  "note.emptyPreview": "Empty note",

  "scope.active": "Active",
  "scope.archived": "Archived",
  "scope.trash": "Trash",
  "scope.archivedEmpty": "No archived notes.",
  "scope.trashEmpty": "Trash is empty.",
  "scope.activeEmpty": "No notes yet.",
  "sort.aria": "Sort order",
  "sort.modified": "Last modified",
  "sort.created": "Created",
  "sort.title": "Title",
  "group.pinned": "PINNED",
  "group.allNotes": "ALL NOTES",

  "noteAction.menu": "Actions",
  "noteAction.pin": "Pin",
  "noteAction.unpin": "Unpin",
  "noteAction.rename": "Rename",
  "noteAction.duplicate": "Duplicate",
  "noteAction.archive": "Archive",
  "noteAction.export": "Export",
  "noteAction.toTrash": "Move to Trash",
  "noteAction.restore": "Restore",
  "noteAction.deleteForever": "Delete forever",
  "trash.emptyTrash": "Empty Trash",
  "trash.deletedAgo": "Deleted {when}",
  "trash.daysLeft": "{count} days left",
  "trash.retention": "Trash items are removed after 30 days.",
  "archive.archivedAgo": "Archived {when}",

  "select.count": "{count} selected",
  "select.cancel": "Cancel",

  "toast.undo": "Undo",
  "toast.trashed": "Note moved to Trash",
  "toast.trashedMany": "{count} notes moved to Trash",
  "toast.archived": "Note archived",
  "toast.archivedMany": "{count} notes archived",
  "toast.restored": "Note restored",
  "toast.deletedForever": "Note permanently deleted",
  "toast.versionRestored": "Version restored",

  "time.justNow": "just now",
  "time.minutesAgo": "{count} min ago",
  "time.hoursAgo": "{count} h ago",
  "time.yesterday": "yesterday",
  "time.daysAgo": "{count} days ago",

  "editor.placeholder": "Start writing, or capture something…",
  "editor.titlePlaceholder": "Untitled note",
  "meta.draft": "DRAFT",
  "meta.empty": "EMPTY",
  "meta.words": "{count} WORDS",

  "empty.title": "No note selected",
  "empty.desc": "Start with a note, dictate something, or paste a screenshot.",

  "dictation.source": "Dictation source",
  "dictation.defaultMic": "Default microphone",
  "dictation.recording": "Recording…",

  "readaloud.label": "Read aloud",
  "readaloud.backend.aria": "Read-aloud backend",
  "readaloud.backend.title": "Voice model / backend",
  "readaloud.voice.aria": "Read-aloud voice",
  "readaloud.emotion.aria": "Read-aloud mood",
  "readaloud.emotion.title": "Mood / emotion (Zonos)",
  "readaloud.exportAudio.aria": "Export audio",
  "readaloud.exportAudio.title": "Save the spoken audio as WAV",
  "readaloud.settings.aria": "More read-aloud settings",

  "readaloud.prepAudio": "Preparing audio … {done}/{total}",
  "readaloud.prepText": "Preparing text … {done}/{total}",
  "readaloud.preparing": "Preparing …",
  "readaloud.paused": "Paused",
  "readaloud.reading": "Reading aloud…",
  "readaloud.prepSuffix": " · preparing {done}/{total}",
  "readaloud.resume": "▶ Resume",
  "readaloud.pause": "⏸ Pause",
  "readaloud.stop": "⏹ Stop",
  "readaloud.cancel": "✕ Cancel",
  "readaloud.voiceLoading":
    "Voice still loading … read-aloud will start automatically once it's ready.",
  "readaloud.sample":
    "This is a short audio sample of the current voice and the chosen settings.",
  "readaloud.exportName": "read-aloud",

  "format.progress": "Formatting … {done}/{total}",

  "ocr.busy": "Recognizing text…",
  "ocr.failed": "Text recognition failed: {error}",
  "ocr.regionFailed": "Region OCR failed: {error}",
  "ocr.title": "Text recognition",
  "ocr.insert": "To note",
  "ocr.insert.title": "Insert the selection – or all the text – into the note",
  "ocr.copy": "Copy",
  "ocr.copy.title": "Copy the selection – or all the text – (Ctrl+C)",
  "ocr.selectAll": "Select all",
  "ocr.selectAll.title": "Select all recognized text (Ctrl+A)",
  "ocr.hint":
    "Select and copy text, or send it to the note — with no selection the whole recognized text is used.",
  "ocr.imageAlt": "Recognized image",

  "history.title": "History",
  "history.empty": "No events yet.",
  "history.changesOnly": "CHANGES ONLY",
  "history.versions": "{count} VERSIONS",
  "history.current": "Current",
  "history.words": "words",
  "history.compare": "COMPARE",
  "history.thisVersion": "This version",
  "history.currentState": "Current",
  "history.readOnly": "READ-ONLY",
  "history.restoreVersion": "Restore this version",
  "history.restoreHint": "Writes as a new, undoable change — non-destructive",
  "history.selectVersion": "Pick a version on the left to see its diff.",
  "history.op.format": "FORMATTED",
  "history.op.manual": "MANUAL",
  "history.op.ocr": "OCR",
  "history.op.dictation": "DICTATION",
  "history.op.restore": "RESTORED",
  "history.op.snapshot": "SNAPSHOT",

  "preview.title": "Formatting – preview",
  "preview.aria": "Formatting preview",
  "preview.original": "Original",
  "preview.formatted": "Formatted",

  "status.saved": "SAVED",
  "status.saving": "SAVING…",
  "status.words": "{count} WORDS",
  "status.readingTime": "{count} MIN READ",

  "models.title": "Manage models",
  "models.tier.bundled": "bundled",
  "models.tier.download": "Download",
  "models.tier.gated": "License required",
  "models.nonCommercial": "non-commercial",
  "models.loading": "loading…",
  "models.setup": "Setup:",
  "models.installed": "installed ✓",
  "models.install": "Install",
  "models.activeProviders": "Active providers",
  "models.confirmNonCommercial":
    "“{name}” is licensed under {license} (not for commercial use). Install anyway?",
  "models.confirmDelete": "Delete “{name}”?",
  "models.installFailed": "Installation failed: {error}",

  "raSettings.title": "Read-aloud settings · {backend}",
  "raSettings.aria": "Read-aloud settings",
  "raSettings.onlySpeed":
    "{backend} uses speed only; the other sound controls exist for Piper and Zonos.",
  "raSettings.speechPrep": "Prepare for speech (LLM)",
  "raSettings.speechPrepHint":
    "Rewrites the text into fluent sentences before reading — better quality, but a short delay.",
  "raSettings.previewing": "▶ Sample playing …",
  "raSettings.preview": "▶ Play sample",
  "slider.speed": "Speed",
  "slider.speed.hint": "Speaking rate",
  "slider.expressiveness": "Expressiveness",
  "slider.expressiveness.hint": "Timbre variation (noise_scale)",
  "slider.cadence": "Cadence",
  "slider.cadence.hint": "Length variation (noise_w)",
  "slider.sentenceSilence": "Sentence pause",
  "slider.sentenceSilence.hint": "Seconds of silence after each sentence",
  "slider.intonation": "Intonation",
  "slider.intonation.hint": "Liveliness of emphasis (monotone ↔ lively)",
  "slider.brightness": "Brightness",
  "slider.brightness.hint": "Treble share (warmer ↔ brighter)",

  "emotion.neutral": "Neutral",
  "emotion.happy": "Happy",
  "emotion.lively": "Lively",
  "emotion.surprised": "Surprised",
  "emotion.calm": "Calm",
  "emotion.sad": "Sad",
  "emotion.fearful": "Fearful",
  "emotion.angry": "Angry",
  "emotion.disgust": "Disgusted",

  "settings.title": "Settings",
  "settings.tab.models": "Models",
  "settings.tab.readaloud": "Read aloud",
  "settings.tab.dictation": "Dictation",
  "settings.tab.appearance": "Appearance",
  "settings.tab.about": "About",
  "settings.models.catalogTitle": "Model catalog",
  "settings.models.catalogHint":
    "Models run fully on-device. “License required” means non-commercial or an acceptance step before download.",
  "settings.models.group.voices": "Voices (TTS)",
  "settings.models.group.runtimes": "Multilingual runtimes",
  "settings.models.group.other": "Other models",
  "settings.models.providersTitle": "Active providers & status",
  "settings.models.empty": "No models in the catalog.",
  "settings.models.size": "Size",
  "settings.models.setupHint": "Install via setup script:",
  "settings.readaloud.voiceTitle": "Voice & backend",
  "settings.readaloud.tuningTitle": "Sound & speed",
  "settings.readaloud.prepTitle": "Speech preparation",
  "settings.readaloud.intro":
    "These controls apply to the active backend ({backend}). Pick voice and backend in the read bar.",
  "settings.dictation.sourceTitle": "Capture source",
  "settings.dictation.source": "Source",
  "settings.dictation.loopback": "System audio (loopback)",
  "settings.dictation.languageTitle": "Language",
  "settings.dictation.language": "Dictation language",
  "settings.dictation.langAuto": "Automatic",
  "settings.dictation.langDe": "German",
  "settings.dictation.langEn": "English",
  "settings.dictation.vadTitle": "Detection & level",
  "settings.dictation.useSilero": "Silero VAD",
  "settings.dictation.useSileroHint": "More precise speech-pause detection (a little more compute).",
  "settings.dictation.autoGain": "Automatic gain",
  "settings.dictation.autoGainHint": "Adjusts the level to the loudness automatically.",
  "settings.dictation.gain": "Microphone gain",
  "settings.appearance.themeTitle": "Theme",
  "settings.appearance.themeLight": "Light",
  "settings.appearance.themeDark": "Dark",
  "settings.appearance.themeSystem": "System",
  "settings.appearance.languageTitle": "Language",
  "settings.appearance.editorTitle": "Editor",
  "settings.appearance.fontSize": "Font size",
  "settings.appearance.contentWidth": "Content width",
  "settings.about.tagline": "A privacy-friendly, fully on-device notes app.",
  "settings.about.version": "Version",
  "settings.about.licenseTitle": "License",
  "settings.about.licenseBody":
    "ExoQuill is licensed under GPL-3.0. Model weights are loaded at runtime and carry their own licenses.",
  "settings.about.componentsTitle": "On-device components in use",
  "settings.about.creditsTitle": "Open-source credits",
  "settings.about.creditsBody":
    "Whisper.cpp · llama.cpp · Tesseract · Piper · Tiptap · Tauri — thanks to all projects.",
  "settings.feature.stt": "Speech recognition",
  "settings.feature.ocr": "Text recognition",
  "settings.feature.formatter": "Formatting",
  "settings.feature.tts": "Speech synthesis",

  "palette.placeholder": "Search commands …",
  "palette.empty": "No matches.",
  "palette.group.actions": "Actions",
  "palette.group.view": "View & settings",
  "palette.cmd.settings": "Open settings",
  "palette.cmd.shortcuts": "Show keyboard shortcuts",
  "palette.cmd.theme": "Toggle theme",
  "palette.cmd.lang": "Switch language",
  "palette.cmd.palette": "Command palette",
  "palette.cmd.import": "Import file",
  "shortcuts.title": "Keyboard shortcuts",
  "shortcuts.group.general": "General",
  "shortcuts.group.actions": "Note actions",
  "shortcuts.newNote": "New note",
  "selection.newNote": "As new note",
  "selection.exportName": "Selection",

  "region.hint": "Drag a region · Esc cancels",
};

const tables: Record<Lang, Record<TranslationKey, string>> = { de, en };

function detect(): Lang {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === "de" || stored === "en") return stored;
  return navigator.language?.toLowerCase().startsWith("de") ? "de" : "en";
}

let current: Lang = detect();
document.documentElement.lang = current;

const listeners = new Set<() => void>();

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

export function getLang(): Lang {
  return current;
}

export function setLang(lang: Lang): void {
  if (lang === current) return;
  current = lang;
  localStorage.setItem(STORAGE_KEY, lang);
  document.documentElement.lang = lang;
  listeners.forEach((l) => l());
}

type Vars = Record<string, string | number>;

/** Translate `key` in `lang`, interpolating `{name}`-style placeholders. Falls
 *  back to the German string, then the raw key. */
export function translate(lang: Lang, key: TranslationKey, vars?: Vars): string {
  let s: string = tables[lang][key] ?? de[key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.split(`{${k}}`).join(String(v));
    }
  }
  return s;
}

export interface I18n {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: TranslationKey, vars?: Vars) => string;
}

/** Subscribe a component to the active language. Re-renders on a switch. */
export function useI18n(): I18n {
  const lang = useSyncExternalStore(subscribe, getLang, getLang);
  return {
    lang,
    setLang,
    t: (key, vars) => translate(lang, key, vars),
  };
}
