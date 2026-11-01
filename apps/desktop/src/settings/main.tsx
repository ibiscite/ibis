import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setPlatform } from "../nativeAppearance";
import { settingsApi } from "./api";
import { SettingsApp } from "./SettingsApp";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider } from "../i18n";
import english from "../i18n/en-US.ftl?raw";
import "../tokens.css";
import "../ui/Foundation.css";

setPlatform();
// `pnpm build:pseudo` exercises the same General panel with expanded labels.
const messages = import.meta.env.MODE === "pseudolocale" ? createInterfaceMessages(english, true) : englishMessages;
const params = new URLSearchParams(window.location.search);
const api = import.meta.env.DEV && params.get("demo") === "1" ? (await import("./demo")).demoApi() : settingsApi;
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
