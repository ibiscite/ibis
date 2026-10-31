import { Component, createRef, type ReactNode } from "react";

type Props = { children: ReactNode };
type Place = { row: HTMLElement; owner: HTMLElement; top: number; noticeHeight: number };

/** Preserve browsing place across notices and scroll-owner changes. The commit
 * snapshot reads the old DOM before layout can clamp its scroll offset, even
 * when the browser has not delivered the person's latest scroll event yet.
 * This geometry only anchors rows; it never chooses or changes a window size. */
export class PickerScroll extends Component<Props> {
  private middle = createRef<HTMLDivElement>();
  private anchor: Place | null = null;
  private resized: ResizeObserver | null = null;

  private capture = (): Place | null => {
    const middle = this.middle.current;
    if (!middle?.isConnected) return null;
    const area = middle.querySelector<HTMLElement>(":scope > .results-area");
    // The first row anchors the whole unchanged list, even when offscreen.
    // Its DOM identity prevents an earlier search from moving replacement rows.
    const row = area?.querySelector<HTMLElement>('[role="option"]');
    const owner = [area, middle].find((part) => part && /auto|scroll/.test(getComputedStyle(part).overflowY));
    if (!row || !owner) return null;
    const noticeHeight = [...middle.querySelectorAll(":scope > .notices")]
      .reduce((height, part) => height + part.getBoundingClientRect().height, 0);
    return { row, owner, top: row.getBoundingClientRect().top, noticeHeight };
  };

  private restore = (before: Place | null) => {
    let after = this.capture();
    if (before && after?.row === before.row && (before.owner !== after.owner || before.noticeHeight !== after.noticeHeight)) {
      // The former owner's offset may already be clamped by the new layout.
      // Transfer the visual position before paint, independently of selection.
      if (before.owner !== after.owner) before.owner.scrollTop = 0;
      after.owner.scrollTop += after.row.getBoundingClientRect().top - before.top;
      after = this.capture();
    }
    this.anchor = after;
  };

  private update = () => { this.restore(this.anchor); };
  private scrolled = (event: Event) => {
    if (event.target === this.anchor?.owner) this.update();
  };

  componentDidMount() {
    const middle = this.middle.current!;
    this.anchor = this.capture();
    this.resized = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(this.update);
    this.resized?.observe(middle);
    window.addEventListener("resize", this.update);
    middle.addEventListener("scroll", this.scrolled, true);
  }

  getSnapshotBeforeUpdate() { return this.capture(); }

  componentDidUpdate(_props: Props, _state: unknown, snapshot: Place | null) {
    this.restore(snapshot);
  }

  componentWillUnmount() {
    this.resized?.disconnect();
    window.removeEventListener("resize", this.update);
    this.middle.current?.removeEventListener("scroll", this.scrolled, true);
    this.anchor = null;
  }

  render() {
    return <div className="picker-content scroll-area" ref={this.middle}>{this.props.children}</div>;
  }
}
