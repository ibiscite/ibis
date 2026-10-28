import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setPlatform } from "../nativeAppearance";
import { settingsApi } from "./api";
import { SettingsApp } from "./SettingsApp";
import "../tokens.css";

setPlatform();
const root = document.getElementById("settings");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <SettingsApp api={settingsApi} initialPage={new URLSearchParams(window.location.search).get("page")} />
    </StrictMode>,
  );
}
