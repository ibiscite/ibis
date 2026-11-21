// The Copied or Pasted notice window. It shows the message it is sent, fades out by
// itself and never takes focus; the app closes or hides the window.
import type { NoticeMessage } from "./api";
import { commands, DEFAULT_PREFERENCES } from "./bindings";
import { events, listening } from "./nativeEvents";
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
  const render = renderNotice(root, platform, () => { void commands.changeSuggestedFormat().catch(() => undefined); });
  // Ibis's own Light or Dark, as the picker shows it; System follows the Mac.
  const show = (message: NoticeMessage) => {
    document.documentElement.dataset.appearance = message.appearance ?? DEFAULT_PREFERENCES.appearance;
    render(readNotice(message));
  };
  const failed = () => console.error(say("copied-notice-notice-failed"));
  // Listen first, then ask for the text in case the event came before the page.
  listening(events.onNoticeLeave, () => leaveNotice(root), { failed });
  listening(events.onNoticeShow, show, { failed, ready: () => {
    commands.copiedNotice().then((message) => { if (message !== null) show(message); }).catch(failed);
  } });
}
