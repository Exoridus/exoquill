# Design-/Feature-Auftrag für ExoQuill (neue Claude-Session)

> Diesen Text in einer **neuen Chat-Session** derselben Projektmappe einfügen.
> Er ist als kompletter Auftrag formuliert und enthält den nötigen Projektkontext.

---

## Rolle & Kontext

Du arbeitest an **ExoQuill**, einer datenschutzfreundlichen, **vollständig on-device**
Notiz-App. Alles läuft lokal, ohne Cloud: Diktat (Whisper), Texterkennung
(Tesseract), Formatierung (llama.cpp) und Vorlesen (Piper/XTTS/Zonos).

**Tech-Stack**
- **Desktop-Shell:** Tauri v2 (Windows-first), Lizenz **GPL-3.0-only**.
- **Frontend:** React 18 + TypeScript + Vite, Editor auf Basis von **Tiptap**
  (Markdown als Speicherformat). Styles in handgeschriebenem CSS mit
  CSS-Variablen (Light/Dark-Theme).
- **Backend:** Rust-Workspace.
  - `crates/exoquill-core` – Domänenmodell (`note.rs`), Jobs, Events, Clock.
  - `crates/exoquill-db` – SQLite-Persistenz (Notizen, `note_events`, `settings`).
  - `crates/exoquill-ai` – KI-Provider (STT/OCR/Formatter/TTS) + Sidecars.
  - `crates/exoquill-audio`, `crates/exoquill-capture` – Audio/Screen-Capture.
  - `apps/desktop/src-tauri` – Tauri-Commands (`notes.rs`, `jobs.rs`, `models.rs`,
    `dictation.rs`, `lib.rs`).

**Arbeitsteilung:** Normalerweise besitzt der Nutzer UI/Branding, Claude die
Internals. **Für diesen Auftrag bekommst du ausdrücklich volle gestalterische
Freiheit** für UI, UX, QoL und Features – Branding-Grundelemente (Wortmarke
„exoquill“, grüner Akzent, Logo-Mark) bitte respektieren bzw. nur behutsam
weiterentwickeln.

**Sprache:** Mit dem Nutzer immer auf **Deutsch** kommunizieren (Code/Identifier
im Original). Vollständige deutsche Orthografie inkl. Umlaute/ß.

---

## Was gerade frisch umgebaut wurde (NICHT rückgängig machen)

Diese Änderungen sind die Ausgangsbasis – darauf aufbauen:

1. **i18n (DE/EN):** `apps/desktop/src/lib/i18n.ts` ist ein leichtgewichtiges
   Lokalisierungssystem (`useI18n()`-Hook + `translate()`), persistiert die Sprache
   in `localStorage`, Default nach Browser-Sprache. **Jeder neue UI-Text muss über
   diese Tabellen (de/en) laufen** – keine hartkodierten Strings mehr.
2. **Eine einzige Toolbar:** Die frühere oberste Leiste (`Topbar`) wurde entfernt.
   Modelle-Button, Sprach-Umschalter (Globus + DE/EN) und Theme-Toggle leben jetzt
   rechts in `components/ActionBar.tsx`; links die notizbezogenen Aktionen. Die
   Wortmarke sitzt jetzt im Sidebar-Kopf (`components/Sidebar.tsx`).
3. **Auto-Titel:** Notiztitel folgen automatisch dem Inhalt (erste sinnvolle
   Zeile), solange der Nutzer den Titel nicht selbst gesetzt hat. Backend:
   `Note.title_auto` (in `exoquill-core/note.rs` + `exoquill-db`), Regeln in
   `Database::update_note`.
4. **OCR-Auswahl gefixt:** `components/OcrOverlay.tsx` rekonstruiert markierten Text
   aus den Wort-Boxen (Leerzeichen/Zeilenumbrüche) und bietet „Alles auswählen“
   (Strg+A).

**Vorhandene Systeme zum Wiederverwenden:**
- Theme: `hooks/useTheme.ts` (`data-theme`-Attribut, persistiert).
- Job-Queue + Event-Bus: `backend-event` (`job_updated`, `notes_changed`),
  `model_progress`. Schwere Aktionen laufen als Jobs (siehe `jobs.rs`).
- Verlauf: Tabelle `note_events` speichert pro Operation `raw_text`,
  `processed_text`, `operation`, `provider_id`, … – Command `list_note_events`.
- Soft-Delete: `Note.deleted_at` existiert bereits (aktuell kein UI dafür).
- Pin/Archiv: `Note.pinned` und `Note.archived` existieren in Schema + `NoteUpdate`
  (Pin wird sortiert; Archiv hat noch **kein** UI und wird im List-Query noch nicht
  gefiltert).
- API-Wrapper: `apps/desktop/src/lib/api.ts` (typed `invoke`-Wrapper).

---

## Auftrag – vier Bereiche

> Liefere **echte Implementierung** (Komponenten, CSS, ggf. neue Tauri-Commands +
> Rust-Logik + Tests), nicht nur Mockups. Arbeite inkrementell und halte die
> bestehenden IPC-Verträge stabil bzw. erweitere sie sauber (serde camelCase,
> Typen in `lib/types.ts` spiegeln). Alles bleibt offline/on-device.

### 1. Notiz-Verwaltung & Editier-Historie

Gestalte das **Verwalten, Bearbeiten, Löschen, Archivieren, Pinnen** von Notizen
neu und vollständig:

- **Pinnen:** Toggle in Sidebar-Item und/oder Kontextmenü; visuell klar (Pin-Icon,
  Gruppierung „Angeheftet“). Backend kann `pinned` schon.
- **Archivieren:** UI zum Archivieren/Wiederherstellen + eigene Ansicht/Filter
  („Aktiv | Archiviert | Papierkorb“). **Backend-Arbeit nötig:** `list_notes`/
  `search_notes` filtern Archiv aktuell nicht – Query/Command anpassen
  (z. B. Parameter `scope`).
- **Löschen:** Soft-Delete (`deleted_at`) existiert. Baue einen echten
  **Papierkorb** mit Wiederherstellen + endgültigem Löschen (neuer Command für
  Hard-Delete + ggf. Aufräum-Routine). Bestätigungen mit Undo-Toast statt nur
  `window.confirm`.
- **Bearbeiten/Organisation:** Mehrfachauswahl, Sortierung (zuletzt geändert /
  erstellt / Titel), evtl. Tags/Ordner als optionales Feature (Schema-Erweiterung
  mit Migration, Muster in `exoquill-db` beachten: `SCHEMA_VERSION` + `migrate()`).
- **Kontextmenü** pro Notiz (Rechtsklick) mit allen Aktionen.

**Editier-Historie „nur bei Diffs“, mit Sprung + Diff gegen aktuellen Stand:**

- Heute existiert nur ein metadaten-orientierter Verlauf (`note_events`) mit
  `raw_text`/`processed_text` für **Operationen** (Format/OCR/Diktat). Manuelles
  Tippen ist nicht versioniert.
- Entwirf eine echte **Versions-/Diff-Historie**:
  - **Entscheide & dokumentiere** den Ansatz: entweder (a) auf `note_events`
    aufbauen (Snapshots je Operation) oder (b) eine **Snapshot-/Versionstabelle**
    ergänzen (z. B. periodische/abgegrenzte Inhalts-Snapshots), oder eine
    Kombination. Empfehlung beim Nutzer einholen, wenn größere Schema-Änderung.
  - **„nur bei Diffs“:** Im Verlauf nur Einträge zeigen, bei denen sich der Inhalt
    tatsächlich geändert hat (kein Rauschen durch No-Op-Saves).
  - **Diff-Ansicht:** Gegenüberstellung früher ↔ aktuell (zeilen-/wortweise,
    Markdown-bewusst). Es gibt bereits ein Vorschau-Panel
    (`.preview-cols`/`.preview-col` in `styles/app.css`) als Stilreferenz.
  - **Springen / Wiederherstellen:** Zu einer Version springen (read-only Vorschau)
    und „diese Version wiederherstellen“ (schreibt als neue, undobare Änderung –
    nicht destruktiv).

### 2. Aktions-Handling für ganze Notiz **und** Auswahl

Gestalte das Zusammenspiel von **OCR, Format, Export, Import, Diktat (Dictate),
Vorlesen (Read)** neu – jeweils sauber **für die gesamte Notiz** *und* **für eine
Auswahl innerhalb der Notiz**.

- **Status heute:**
  - Format & Read berücksichtigen bereits eine Auswahl (`selectionText`), sonst
    ganze Notiz.
  - OCR fügt am Cursor ein bzw. legt neue Notiz an.
  - Diktat: ersetzt Auswahl / fügt am Cursor ein.
  - **Export** existiert nur für die **ganze** Notiz als Markdown
    (`export_note`), nicht für Auswahl.
  - **Import** existiert **noch gar nicht** → neuer Tauri-Command nötig
    (Datei-Dialog, `.md`/`.txt` → neue Notiz oder am Cursor einfügen; ggf.
    Mehrfach-Import).
- **Ziel:** Ein **einheitliches, entdeckbares Interaktionsmodell**:
  - **Auswahl-Bubble-Menü** (Tiptap BubbleMenu o. Ä.), das bei Markierung
    erscheint und kontextbezogen Format/Read/Export-Auswahl/Diktat-ersetzt/„als
    neue Notiz“ anbietet.
  - Klare Trennung „wirkt auf Auswahl“ vs. „wirkt auf ganze Notiz“ (Label/Tooltip/
    State), damit nie unklar ist, was passiert.
  - **Export der Auswahl** (Markdown/Plaintext) ergänzen; Export-Formate erweitern
    (z. B. `.txt`, evtl. `.pdf`/`.html` – nur wenn offline gut machbar).
  - Fortschritts-/Abbrechen-UX für lange Läufe vereinheitlichen (heute mehrere
    `dictation-bar`-Varianten in `App.tsx`).
  - Tastenkürzel für alle Aktionen, konsistent dokumentiert.

### 3. Neues Settings-Fenster

Ein vollwertiges, gut strukturiertes **Einstellungs-Fenster** (Dialog oder eigene
Route), das **alles** bündelt. Heute verstreut: Modell-Manager
(`components/ModelManager.tsx`), Vorlese-Einstellungen
(`components/ReadAloudSettings.tsx`), Theme-Toggle, Sprach-Umschalter.

Inhalte (als Tabs/Sektionen):

- **Modelle & Runtimes:** Katalog installieren/löschen, Download-Fortschritt,
  Lizenz-/Tier-Badges (vorhanden via `list_catalog`/`install_model`/`delete_model`,
  `model_progress`), aktive Provider + Status/Health (`list_model_info`). Pfade,
  Speicherort, belegter Speicher, „nicht-kommerziell“-Gates.
- **Vorlesen/Audio:** Backend-/Stimmen-Wahl, Tuning (vorhanden in
  `ReadAloudSettings`), Sprach-Aufbereitung (LLM) erklären.
- **Diktat:** Quelle/Loopback, Sprache (`languageMode`), VAD-/Gain-Optionen
  (siehe `startDictation`-Optionen in `api.ts`).
- **Darstellung:** Theme (Light/Dark/System), Sprache (DE/EN), Editor-Optionen
  (Schriftgröße/Breite), Akzentfarbe.
- **About:** Version, Lizenz (GPL-3.0), verwendete Open-Source-Modelle/Runtimes
  + deren Lizenzen, Links/Credits.
- **Updates:** Update-Seite/Feature (Tauri-Updater prüfen; falls noch nicht
  konfiguriert: UI + Backend-Anbindung entwerfen, mind. „nach Updates suchen“,
  Versionsanzeige, Changelog-Hook). Offline-tauglich/optional halten.

Persistenz: Es gibt bereits eine `settings`-Tabelle (`get_setting`/`set_setting`,
JSON-Werte) – nutze sie für serverseitig relevante Settings; reine UI-Settings
dürfen in `localStorage` bleiben (Konsistenz mit bestehendem Code, z. B.
`tts-voice`, `tts-tuning`, `exoquill-theme`, `exoquill-lang`).

### 4. Freie Quality-of-Life- & UX-/Performance-Verbesserungen

Volle kreative Freiheit für sinnvolle Verbesserungen, z. B.:

- Command-Palette (Strg+K), globale Shortcuts-Übersicht, bessere Suche
  (Treffer-Hervorhebung, Filter).
- Editor-QoL: Slash-Commands, bessere Markdown-Toolbar/BubbleMenu, Outline,
  Wortzähler/Lesezeit, Auto-Save-Indikator-Feinschliff.
- Performance: virtualisierte Notizliste bei vielen Notizen, Diff-Berechnung
  effizient, Editor-Remounts vermeiden (`reloadKey`-Muster in `App.tsx` ansehen).
- A11y: Fokus-Management, Tastaturbedienung, ARIA, Kontraste.
- Onboarding/Empty-States, Toaster/Benachrichtigungen statt blockierender
  `window.confirm`/`alert`.

Größere Architektur-/Schema-Eingriffe oder neue Abhängigkeiten vorher kurz mit dem
Nutzer abstimmen.

---

## Leitplanken (Definition of Done)

- **Offline/On-device** bleibt Pflicht – keine externen Netzwerk-Calls für
  Kernfunktionen.
- **Zweisprachig:** alle neuen Texte über `lib/i18n.ts` (de + en gepflegt).
- **Theme-fähig:** CSS über bestehende Variablen, Light **und** Dark testen.
- **IPC sauber:** neue Commands in `src-tauri` registrieren (`lib.rs`
  `invoke_handler`), Typen in `lib/types.ts` spiegeln (serde `camelCase`).
- **DB-Migrationen** über `SCHEMA_VERSION` + idempotentes `migrate()` (Muster in
  `exoquill-db/src/lib.rs`), bestehende Daten nicht zerstören.
- **Tauri-Threading:** schwere IPC-Commands als sync `fn` (laufen off-main-thread)
  bzw. über die Job-Queue – nicht `async` blockierend.
- **Tests** für Backend-Logik (siehe vorhandene `#[cfg(test)]`-Module) und
  Typecheck grün: `cd apps/desktop && npx tsc --noEmit`, `cargo test --workspace`.
- Inkrementell vorgehen, Zwischenstände erklären, Empfehlungen aktiv geben.

## Erste Schritte (Vorschlag)

1. Repo-Kontext sichten: `docs/decisions.md`, `docs/roadmap.md`, `App.tsx`,
   `components/`, `exoquill-db/src/lib.rs`, `exoquill-core/src/note.rs`,
   `src-tauri/src/{notes,jobs,models}.rs`.
2. Kurzes Umsetzungskonzept je Bereich vorschlagen (inkl. nötiger Schema-/IPC-
   Änderungen) und offene Design-Entscheidungen mit dem Nutzer klären.
3. Bereich für Bereich umsetzen, mit Light/Dark- und DE/EN-Check.
