# Settings-Fenster (Bereich 3) + TTS-Redesign + Polished UX — Design

- Status: **in Arbeit** · Datum: 2026-06-23
- Umsetzt: `docs/design-prompt.md` Bereiche 2–4 (Bereich 1 = D12 ist fertig)
- Bezieht sich auf: D9 (Model-Manager), D10/D11 (TTS-Backends), `docs/decisions.md`

## Ziel & Auftrag

Der Nutzer war frustriert: Vom Design-System (Direction B) wurde nur **Bereich 1**
umgesetzt; der Modell-Overlay (`ModelManager.tsx`) recycelt nur die History-Panel-
Styles und wirkt „hässlich, unordentlich, kaum benutzbar". Die vorgegebene TTS-
Modell-Liste (D11, 7 Ränge) ist nur zu 3/7 verdrahtet, und die TTS-Auswahl ist auf
drei Orte verstreut (Toolbar-Backend-Dropdown, ReadAloudSettings, ModelManager).

Auftrag (autonom, ohne viel Rückfragen): **Bereiche 2–4 umsetzen**, Fokus zuerst auf
**Modelle + Einstellungen + spürbar besseres UX**. **Updates-Tab entfällt.**

## Entscheidungen (autonom getroffen)

1. **Ein Settings-Fenster** ersetzt die Overlays `ModelManager` und `ReadAloudSettings`.
   Eigenes, poliertes Chrome (kein recyceltes `history-panel`): Backdrop + Fenster
   (~880×600) mit **Tab-Schiene links** (Icon + Label) und **Body rechts**.
2. **Tabs:** Modelle · Vorlesen · Diktat · Darstellung · Über. (Kein Updates.)
3. **Einstiegspunkte:** ActionBar-Button „Modelle" → öffnet Settings auf Tab *Modelle*;
   Vorlese-Zahnrad → Settings auf Tab *Vorlesen*. Schnelle Theme-/Sprache-Toggles
   bleiben in der ActionBar (Komfort); volle Kontrollen zusätzlich in *Darstellung*.
4. **Toolbar bleibt schlank:** Backend- + Stimmen-Picker bleiben kontextuell in der
   Lese-Leiste (D10); Tuning, Emotion, Speech-Prep wandern in Tab *Vorlesen*.
5. **Persistenz:** reine UI-Settings über `localStorage` (konsistent mit
   `tts-voice`, `tts-tuning`, `exoquill-theme`, `exoquill-lang`). Keine neuen Rust-
   Commands für Bereich 3. Neu: `exoquill-editor-prefs` (Schriftgröße/Breite),
   Theme bekommt `system`-Modus, `exoquill-dictation-opts` (autoGain/gain/useSilero).
6. **Theme `system`:** `useTheme` wird um „System folgen" (prefers-color-scheme) erweitert.

## Parallelisierungs-Strategie (kollisionsfrei)

- **Spine-Dateien besitzt der Orchestrator (ich):** `App.tsx`, `i18n.ts`, `types.ts`,
  `api.ts`, `styles/*.css`, `hooks/useTheme.ts`, `lib.rs`, `models.json`, neue Rust-Module-
  Verdrahtung. Diese editiere ich seriell.
- **Agents bauen nur NEUE, in sich geschlossene Dateien** gegen feste Verträge:
  je eine Tab-Komponente unter `components/settings/`, später Command-Palette,
  Bubble-Menu, neues Rust-Provider-Modul. Agents editieren KEINE Spine-Datei.
- **i18n + CSS-Vokabular definiere ich vorab** (siehe unten); Agents nutzen exakt
  diese Keys/Klassen und melden fehlende per Rückgabe (ich ergänze).

## Architektur — Settings-Fenster

`components/SettingsWindow.tsx` (Shell, ich):
- State: `activeTab` (Default per Prop `initialTab`).
- Rendert Backdrop + Fenster; links Tab-Schiene, rechts den aktiven Tab.
- Importiert die Tab-Komponenten und reicht ihnen ihre Props durch.

Tab-Komponenten (`components/settings/`, je eigene Datei — Agents):

### `ModelsTab.tsx`
```ts
interface ModelsTabProps {
  items: CatalogItem[];
  providers: ModelInfo[];
  progress: Record<string, ModelProgress>;
  busyId: string | null;
  onInstall: (item: CatalogItem) => void;
  onDelete: (item: CatalogItem) => void;
}
```
Inhalt: Katalog als **gruppierte, polierte Karten** (Gruppierung nach `kind`/Sprache:
TTS-Stimmen, TTS-Runtimes, …), je Karte: Name, Sprache, Tier-Badge, Lizenz-Badge,
NC-Gate-Badge, Größe, Status (Installieren / installiert ✓ / Setup-Hinweis / Löschen),
Download-Fortschrittsbalken. Darunter Sektion **Aktive Provider & Health**
(`providers`) mit Status-Punkten. Quelle der bisherigen Logik: `ModelManager.tsx`.

### `ReadAloudTab.tsx`
```ts
interface ReadAloudTabProps {
  backendLabel: string; isPiper: boolean; isZonos: boolean;
  tuning: TtsTuning; onChange: (t: TtsTuning) => void;
  speechPrep: boolean; onSpeechPrepChange: (v: boolean) => void;
  onPreview: () => void; previewing: boolean;
}
```
Inhalt: identische Tuning-Slider/Logik wie `ReadAloudSettings.tsx` (Quelle), nur ohne
eigenes Overlay-Chrome — als Settings-Sektionen. Speech-Prep-Toggle + Probe-Button.

### `DictationTab.tsx`
```ts
interface DictationTabProps {
  sources: CaptureSource[];
  sourceName: string | null;
  onSourceChange: (name: string | null) => void;
  opts: { autoGain: boolean; gain: number; useSilero: boolean };
  onOptsChange: (o: { autoGain: boolean; gain: number; useSilero: boolean }) => void;
  languageMode: string; onLanguageModeChange: (m: string) => void;
}
```
Inhalt: Quelle/Loopback-Auswahl, Sprache (`de`/`en`/`auto`), VAD (Silero an/aus),
Auto-Gain-Toggle + Gain-Slider. Keine neuen Backend-Calls; Werte fließen in
`startDictation`-Optionen.

### `AppearanceTab.tsx`
```ts
interface AppearanceTabProps {
  theme: "light" | "dark" | "system"; onThemeChange: (t: "light"|"dark"|"system") => void;
  lang: Lang; onLangChange: (l: Lang) => void;
  prefs: { fontScale: number; contentWidth: number };
  onPrefsChange: (p: { fontScale: number; contentWidth: number }) => void;
}
```
Inhalt: Theme (Hell/Dunkel/System als Segmented-Control), Sprache (DE/EN), Editor-
Schriftgröße (Slider, setzt `--editor-font-scale`), Inhaltsbreite (Slider, setzt
`--editor-content-width`).

### `AboutTab.tsx`
```ts
interface AboutTabProps { version: string; providers: ModelInfo[]; }
```
Inhalt: Wortmarke + Version, Lizenz GPL-3.0 (kurze Erklärung), Liste der genutzten
Open-Source-Modelle/Runtimes mit ihren Lizenzen (aus `providers` + statische Liste),
Credits/Links. Hinweis: Chatterbox-Wasserzeichen (D11) wenn vorhanden.

## i18n — neue Keys (ich lege sie an; Agents nutzen sie)

Namespace `settings.*`:
`settings.title`, `settings.tab.models|readaloud|dictation|appearance|about`,
`settings.models.*` (catalogTitle, providersTitle, storageTitle, group.voices,
group.runtimes, group.other, status.ready, etc.), `settings.readaloud.*`,
`settings.dictation.*` (source, loopback, language, vad, autoGain, gain, langAuto),
`settings.appearance.*` (theme, themeLight, themeDark, themeSystem, language,
fontSize, contentWidth), `settings.about.*` (version, license, licenseBody,
components, credits). Bestehende `models.*`, `raSettings.*`, `slider.*`, `emotion.*`
bleiben nutzbar und werden wiederverwendet.

## CSS-Vokabular — `styles/settings.css` (ich)

`.settings-backdrop`, `.settings-window`, `.settings-window__rail`, `.settings-tab`
(+`--active`), `.settings-window__body`, `.settings-section`, `.settings-section__title`,
`.settings-section__hint`, `.settings-row`, `.settings-field`, `.settings-field__label`,
`.settings-field__control`, `.settings-field__hint`, `.settings-card`,
`.settings-card__head`, `.settings-card__title`, `.settings-card__meta`,
`.settings-card__actions`, `.settings-card__progress`, `.settings-badge`
(+`--bundled|--download|--gated|--nc`), `.settings-segmented` (+`__option`),
`.settings-provider`, `.settings-provider__status` (+`--ready|--mock|--unavailable`).
Tokens aus `theme.css` (—surface, —border, —accent, —text*, —radius*, —shadow-window).

## Wellen (Reihenfolge)

1. **Settings-Kern** (diese + nächste Schritte): Shell + 5 Tabs + CSS + i18n +
   Integration in `App.tsx`; Overlays ersetzt; Theme-`system`; Typecheck grün.
2. **Polished UX (Bereich 4):** Command-Palette (Strg+K), Shortcut-Übersicht,
   Such-Highlight, Editor-QoL (Wortzähler/Outline), Empty-States, A11y-Feinschliff.
3. **Aktions-Handling (Bereich 2):** `import_note`-Command, Export für Auswahl +
   `.txt`, Tiptap-BubbleMenu, vereinheitlichte Progress/Cancel-Leiste.
4. **TTS-Liste erweitern:** Chatterbox-Provider (MIT, D11 „next to wire"), dann
   Kokoro; `models.json` + Sidecar-Module; im merged Voice-Picker wählbar.

## Definition of Done

- Offline/On-Device bleibt Pflicht; keine externen Calls für Kernfunktionen.
- Alle neuen Texte über `i18n.ts` (de + en gepflegt, Parität erzwungen).
- Light **und** Dark getestet; Tokens statt Hardcodes.
- `cd apps/desktop && npx tsc --noEmit` grün; `cargo test --workspace` grün.
- Tauri-Threading: schwere Commands sync `#[tauri::command(async)]` bzw. `off_thread`.
