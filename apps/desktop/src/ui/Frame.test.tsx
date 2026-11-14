import { StrictMode } from "react";
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { Frame, FrameHeading, FrameNotice, FrameNotices, LineNotice } from "./Frame";
import { NoticeSlot, QuietNotice } from "./NoticeSlot";

const { readFileSync } = await vi.importActual<{ readFileSync: (path: URL, encoding: string) => string }>("node:fs");
const { URL: FileURL } = await vi.importActual<{ URL: typeof URL }>("node:url");
const frameStyles = readFileSync(new FileURL("./Frame.css", import.meta.url), "utf8");
const bandStyles = readFileSync(new FileURL("./Band.css", import.meta.url), "utf8");

afterEach(cleanup);

test("the frame tracks a heading and notices appearing and clearing in the same line", () => {
  const view = (heading: boolean, notices: number) => <StrictMode><style>{frameStyles}{bandStyles}</style><Frame search={null} notice={<>
    {notices > 0 && <LineNotice>First notice</LineNotice>}
    {notices > 1 && <FrameNotice><ul><li>Another notice</li></ul></FrameNotice>}
  </>}><FrameHeading>{heading && <div>Invented results</div>}</FrameHeading></Frame></StrictMode>;
  const { container, rerender } = render(view(false, 0));
  const frame = container.querySelector("main")!;
  const line = frame.querySelector(".ui-notice-line")!;
  expect(frame.hasAttribute("data-heading")).toBe(false);
  expect(frame.hasAttribute("data-notice")).toBe(false);
  expect(getComputedStyle(line).display).toBe("none");
  rerender(view(true, 0));
  expect(frame.hasAttribute("data-heading")).toBe(true);
  expect(line.querySelector(".ui-heading-slot")?.textContent).toBe("Invented results");
  expect(getComputedStyle(line).display).toBe("flex");
  rerender(view(true, 2));
  expect(frame.hasAttribute("data-notice")).toBe(true);
  expect(getComputedStyle(line.querySelector(".ui-heading-slot")!).display).toBe("none");
  rerender(view(true, 1));
  expect(frame.hasAttribute("data-notice")).toBe(true);
  rerender(view(true, 0));
  expect(frame.hasAttribute("data-notice")).toBe(false);
  expect(frame.hasAttribute("data-heading")).toBe(true);
  expect(getComputedStyle(line.querySelector(".ui-heading-slot")!).display).toBe("block");
  rerender(view(false, 0));
  expect(frame.hasAttribute("data-heading")).toBe(false);
  expect(getComputedStyle(line).display).toBe("none");
  expect(frame.querySelector(".ui-notice-line")).toBe(line);
});

test("a notice slot registers only while populated and keeps its node when the empty-state room opens", () => {
  const view = (notice: boolean, rooms: number) => <StrictMode><style>{frameStyles}{bandStyles}</style><Frame search={null}
    notice={<NoticeSlot>{notice && <QuietNotice>Invented update</QuietNotice>}</NoticeSlot>}>
    {rooms > 0 && <FrameNotices />}{rooms > 1 && <FrameNotices />}
  </Frame></StrictMode>;
  const { container, rerender } = render(view(false, 0));
  const frame = container.querySelector("main")!;
  const line = frame.querySelector(".ui-notice-line")!;
  expect(frame.hasAttribute("data-notice")).toBe(false);
  expect(getComputedStyle(line).display).toBe("none");
  rerender(view(true, 0));
  const notice = frame.querySelector(".notice-slot")!;
  expect(frame.hasAttribute("data-notice")).toBe(true);
  expect(getComputedStyle(line).gridArea).toBe("notice");
  rerender(view(true, 2));
  expect(frame.hasAttribute("data-notice-room")).toBe(true);
  expect(getComputedStyle(line).gridArea).toBe("middle");
  expect(frame.querySelector(".notice-slot")).toBe(notice);
  rerender(view(true, 1));
  expect(frame.hasAttribute("data-notice-room")).toBe(true);
  rerender(view(false, 0));
  expect(frame.hasAttribute("data-notice-room")).toBe(false);
  expect(frame.hasAttribute("data-notice")).toBe(false);
  expect(getComputedStyle(line).display).toBe("none");
});
