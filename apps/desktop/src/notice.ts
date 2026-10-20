// The Copied notice window. It shows the text it is sent, fades out by
// itself and never takes focus; the app closes or hides the window.
import { listen } from "@tauri-apps/api/event";
import { commands } from "./bindings";
import { leaveNotice, noticePlatform, renderNotice } from "./copyFeedback";
import { copiedNoticeText } from "./messages";
import "./tokens.css";
import "./notice.css";

const root = document.getElementById("notice");
if (root) {
  const platform = noticePlatform(navigator.platform);
  document.documentElement.dataset.platform = platform;
  // Without a compositor the window is opaque and square (see notice.rs).
  if (new URLSearchParams(window.location.search).get("surface") === "opaque") {
    document.documentElement.dataset.surface = "opaque";
  }
  const show = renderNotice(root, platform);
  // Listen first, then ask for the text in case the event came before the page.
  void listen("notice:leave", () => leaveNotice(root)).catch(() => console.error(copiedNoticeText.noticeFailed));
  void listen<string>("notice:show", (event) => show(event.payload))
    .then(() => commands.copiedNotice())
    .then((text) => { if (text !== null) show(text); })
    .catch(() => console.error(copiedNoticeText.noticeFailed));
}
