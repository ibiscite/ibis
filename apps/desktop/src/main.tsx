import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./ui/Window.css";
import "./ui/Foundation.css";
import { commands } from "./bindings";
import { installNativeAppearance, setPlatform } from "./nativeAppearance";
import { InterfaceMessagesProvider, setInterfaceLanguage } from "./i18n";

setPlatform();
void installNativeAppearance(commands.nativeAppearance);
// The window speaks the app's language before it draws a word; `pnpm
// build:pseudo` expands every label to show clipped or untranslated text.
const preferences = await commands.pickerPreferences().catch(() => null);
const messages = setInterfaceLanguage(preferences?.speaking ?? "en-US", import.meta.env.MODE === "pseudolocale");
const { App } = await import("./App");

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <InterfaceMessagesProvider messages={messages}>
        <App />
      </InterfaceMessagesProvider>
    </StrictMode>,
  );
}
