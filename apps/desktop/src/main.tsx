import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";
import { commands } from "./bindings";
import { installNativeAppearance } from "./nativeAppearance";

void installNativeAppearance(commands.nativeAppearance);

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}
