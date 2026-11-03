import { Component, createRef, useEffect, useRef, type ReactNode } from "react";

import { useFileDrag } from "../../ui/useFileDrag";

type Props = { onFileDrag?: (change: (active: boolean) => void) => Promise<() => void>; presentation?: number; drag?: ReturnType<typeof useFileDrag>; children: ReactNode; citationProblem?: string | null; citationProblemIdentity?: object };
type Place = { node: HTMLElement; owner: HTMLElement; top: number; noticeHeight: number };

/** Preserve browsing place across notices and scroll-owner changes. The commit
 * snapshot reads the old DOM before layout can clamp its scroll offset, even
 * when the browser has not delivered the person's latest scroll event yet.
 * A refusal owns the reading place while present. This geometry never chooses
 * or changes a window size. */
class PickerScrollFrame extends Component<Props> {
  private middle = createRef<HTMLDivElement>();
  private anchor: Place | null = null;
  private resized: ResizeObserver | null = null;

  private capture = (): Place | null => {
    const middle = this.middle.current;
    if (!middle?.isConnected) return null;
    const area = middle.querySelector<HTMLElement>(":scope > .results-area");
    // Preserve the refusal's reading place; otherwise anchor the first row of
    // the unchanged list, even offscreen. DOM identity excludes replaced rows.
    const problem = middle.querySelector<HTMLElement>(":scope > .notices > .citation-problem");
    const node = problem ?? area?.querySelector<HTMLElement>('[role="option"]');
    const owner = problem ? middle : [area, middle].find((part) => part && /auto|scroll/.test(getComputedStyle(part).overflowY));
    if (!node || !owner) return null;
    const noticeHeight = [...middle.querySelectorAll(":scope > .notices")]
      .reduce((height, part) => height + part.getBoundingClientRect().height, 0);
    return { node, owner, top: node.getBoundingClientRect().top, noticeHeight };
  };

  private restore = (before: Place | null) => {
    let after = this.capture();
    if (before && after?.node === before.node && (before.owner !== after.owner || before.noticeHeight !== after.noticeHeight)) {
      // The former owner's offset may already be clamped by the new layout.
      // Transfer the visual position before paint, independently of selection.
      if (before.owner !== after.owner) before.owner.scrollTop = 0;
      after.owner.scrollTop += after.node.getBoundingClientRect().top - before.top;
      after = this.capture();
    }
    this.anchor = after;
  };

  private update = () => { this.restore(this.anchor); };
  private scrolled = (event: Event) => {
    if (event.target === this.anchor?.owner) this.update();
  };

  revealCitationProblem = (notice: string, identity?: object) => {
    if (this.props.citationProblem !== notice || this.props.citationProblemIdentity !== identity) return;
    const middle = this.middle.current;
    const problem = middle?.querySelector<HTMLElement>(":scope > .notices > .citation-problem");
    if (!middle || !problem) return;
    // Reveal the new notice after the commit's layout has settled. Only the
    // middle scroller moves, instantly; Search and its selection keep focus.
    middle.scrollTop = middle.scrollTop + problem.getBoundingClientRect().top - middle.getBoundingClientRect().top - middle.clientTop;
    this.anchor = this.capture();
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
  const frame = useRef<PickerScrollFrame>(null);
  const drag = useFileDrag(props.onFileDrag, props.presentation);
  const { citationProblem, citationProblemIdentity } = props;
  // Arrival has one owner, separate from preserving an existing reading place.
  // Equal wording from a newer format request is still a new notice. Renders
  // and later layout changes for the same notice never repeat this scroll.
  useEffect(() => {
    if (citationProblem) frame.current?.revealCitationProblem(citationProblem, citationProblemIdentity);
  }, [citationProblem, citationProblemIdentity]);
  return <PickerScrollFrame {...props} drag={drag} ref={frame} />;
}
