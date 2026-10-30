import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { setPlatform } from "../nativeAppearance";
import { settingsApi } from "./api";
import { SettingsApp } from "./SettingsApp";
import { createInterfaceMessages, englishMessages, InterfaceMessagesProvider } from "../i18n";
import english from "../i18n/en-US.ftl?raw";
import "../tokens.css";

setPlatform();
// `pnpm build:pseudo` exercises the same General panel with expanded labels.
const messages = import.meta.env.MODE === "pseudolocale" ? createInterfaceMessages(english, true) : englishMessages;
const root = document.getElementById("settings");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <InterfaceMessagesProvider messages={messages}>
        <SettingsApp api={settingsApi} initialPage={new URLSearchParams(window.location.search).get("page")} />
      </InterfaceMessagesProvider>
    </StrictMode>,
  );
}
