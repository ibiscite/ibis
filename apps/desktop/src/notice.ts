// The Copied or Pasted notice window. It shows the message it is sent, fades out by
// itself and never takes focus; the app closes or hides the window.
import { listen } from "@tauri-apps/api/event";
import type { NoticeMessage } from "./api";
import { commands } from "./bindings";
import { leaveNotice, noticePlatform, renderNotice } from "./copyFeedback";
import { copiedNoticeText } from "./messages";
import { setPlatform } from "./nativeAppearance";
import "./tokens.css";
import "./ui/CopyReceipt.css";
import "./notice.css";

const root = document.getElementById("notice");
if (root) {
  const platform = noticePlatform(navigator.platform);
  setPlatform();
  // Without a compositor the window is opaque and square (see notice.rs).
  if (new URLSearchParams(window.location.search).get("surface") === "opaque") {
    document.documentElement.dataset.surface = "opaque";
  }
  const show = renderNotice(root, platform);
  // Listen first, then ask for the text in case the event came before the page.
  void listen("notice:leave", () => leaveNotice(root)).catch(() => console.error(copiedNoticeText.noticeFailed));
  void listen<NoticeMessage>("notice:show", (event) => show(event.payload))
    .then(() => commands.copiedNotice())
    .then((message) => { if (message !== null) show(message); })
    .catch(() => console.error(copiedNoticeText.noticeFailed));
}
