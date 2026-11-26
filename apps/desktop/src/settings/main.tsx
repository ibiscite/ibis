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
const demo = import.meta.env.DEV && params.get("demo") === "1";
const api = demo ? (await import("./demo")).demoApi() : settingsApi;
// Native supplies the process's language, which stays fixed until Ibis restarts.
const language = params.get("language");
const messages = setInterfaceLanguage(language === "de" || language === "ar" ? language : "en-US", import.meta.env.MODE === "pseudolocale");
const root = document.getElementById("settings");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <InterfaceMessagesProvider messages={messages}>
        <SettingsApp api={api} initialPage={demo ? params.get("page") : undefined} />
      </InterfaceMessagesProvider>
    </StrictMode>,
  );
}
