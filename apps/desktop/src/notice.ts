// The Copied or Pasted notice window. It shows the message it is sent, fades out by
// itself and never takes focus; the app closes or hides the window.
import { listen } from "@tauri-apps/api/event";
import type { NoticeMessage } from "./api";
import { commands } from "./bindings";
import { leaveNotice, noticePlatform, readNotice, renderNotice } from "./copyFeedback";
import { say } from "./i18n";
import { setPlatform } from "./nativeAppearance";
import "./tokens.css";
import "./ui/CopyReceipt.css";
import "./ui/CopiedNotice.css";

const root = document.getElementById("notice");
if (root) {
  const platform = noticePlatform(navigator.platform);
  setPlatform();
  // Without a compositor the window is opaque and square (see notice.rs).
  if (new URLSearchParams(window.location.search).get("surface") === "opaque") {
    document.documentElement.dataset.surface = "opaque";
  }
  const render = renderNotice(root, platform);
  // Ibis's own Light or Dark, as the picker shows it; System follows the Mac.
  const show = (message: NoticeMessage) => {
    document.documentElement.dataset.appearance = message.appearance ?? "system";
    render(readNotice(message));
  };
  // Listen first, then ask for the text in case the event came before the page.
  void listen("notice:leave", () => leaveNotice(root)).catch(() => console.error(say("copied-notice-notice-failed")));
  void listen<NoticeMessage>("notice:show", (event) => show(event.payload))
    .then(() => commands.copiedNotice())
    .then((message) => { if (message !== null) show(message); })
    .catch(() => console.error(say("copied-notice-notice-failed")));
}
