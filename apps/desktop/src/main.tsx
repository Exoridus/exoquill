import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/ibm-plex-mono/500.css";
import "./styles/theme.css";
import "./styles/global.css";

import { getCurrentWindow } from "@tauri-apps/api/window";
import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";
import { RegionOverlay } from "./components/RegionOverlay";

// The same bundle serves both windows; route by the Tauri window label so the
// region-OCR overlay renders its selection UI instead of the full app.
const isRegionOverlay = getCurrentWindow().label === "region-overlay";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isRegionOverlay ? <RegionOverlay /> : <App />}</React.StrictMode>,
);
