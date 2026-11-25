import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setPlatform } from "../nativeAppearance";
import { settingsApi } from "./api";
import { SettingsApp } from "./SettingsApp";
import { InterfaceMessagesProvider, setInterfaceLanguage } from "../i18n";
import "../tokens.css";
import "../ui/Foundation.css";

setPlatform();
const params = new URLSearchParams(window.location.search);
const api = import.meta.env.DEV && params.get("demo") === "1" ? (await import("./demo")).demoApi() : settingsApi;
// The app's language, as the picker speaks it; `pnpm build:pseudo` expands every label.
const preferences = await api.pickerPreferences().catch(() => null);
const messages = setInterfaceLanguage(preferences?.speaking ?? "en-US", import.meta.env.MODE === "pseudolocale");
const root = document.getElementById("settings");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <InterfaceMessagesProvider messages={messages}>
        <SettingsApp api={api} initialPage={params.get("page")} />
      </InterfaceMessagesProvider>
    </StrictMode>,
  );
}
