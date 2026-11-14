import { Component, createRef, type ReactNode } from "react";

import { useFileDrag } from "../../ui/useFileDrag";

type Props = { onFileDrag?: (change: (active: boolean) => void) => Promise<() => void>; presentation?: number; drag?: ReturnType<typeof useFileDrag>; children: ReactNode };
type Place = { node: HTMLElement; owner: HTMLElement; top: number };

/** The content slot's one scroller. Notices live in the frame's own line
 * (ui/Frame), so nothing is inserted above the rows; only a window too short
 * for the list's own viewport hands scrolling to the middle region, and then
 * the first row keeps its place. The commit snapshot reads the old DOM before
 * layout can clamp its scroll offset. This geometry never chooses or changes a
 * window size. */
class PickerScrollFrame extends Component<Props> {
  private middle = createRef<HTMLDivElement>();
  private anchor: Place | null = null;
  private resized: ResizeObserver | null = null;

  private capture = (): Place | null => {
    const middle = this.middle.current;
    if (!middle?.isConnected) return null;
    const area = middle.querySelector<HTMLElement>(":scope > .results-area");
    // DOM identity excludes replaced rows.
    const node = area?.querySelector<HTMLElement>('[role="option"]');
    const owner = [area, middle].find((part) => part && /auto|scroll/.test(getComputedStyle(part).overflowY));
    if (!node || !owner) return null;
    return { node, owner, top: node.getBoundingClientRect().top };
  };

  private restore = (before: Place | null) => {
    let after = this.capture();
    if (before && after?.node === before.node && before.owner !== after.owner) {
      // The former owner's offset may already be clamped by the new layout.
      before.owner.scrollTop = 0;
      after.owner.scrollTop += after.node.getBoundingClientRect().top - before.top;
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

  componentDidUpdate(_previous: Props, _state: unknown, snapshot: Place | null) {
    this.restore(snapshot);
  }

  componentWillUnmount() {
    this.resized?.disconnect();
    window.removeEventListener("resize", this.update);
    this.middle.current?.removeEventListener("scroll", this.scrolled, true);
    this.anchor = null;
  }

  render() {
    return <div {...this.props.drag} className="picker-content scroll-area" ref={this.middle}>{this.props.children}</div>;
  }
}

export function PickerScroll(props: Props) {
  const drag = useFileDrag(props.onFileDrag, props.presentation);
  return <PickerScrollFrame {...props} drag={drag} />;
}
