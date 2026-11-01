import type { SearchIcon } from "../bindings";
import { createRef, useLayoutEffect, useRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Density, SearchResult } from "../bindings";
import { PickerHeader } from "../features/picker/PickerHeader";
import { ResultRow } from "../features/picker/ResultList";
import { useInterfaceMessages } from "../i18n";
import pickerStyles from "../styles.css?raw";
import buttonStyles from "../ui/Button.css?raw";

const ignore = () => undefined;
// The real styles stay isolated from Settings, in the app's own order. Each host inherits the current
// appearance or owns its explicitly requested scheme, never the system default.
const styles = (buttonStyles + pickerStyles).replace(/@import[^;]+;/g, "")
  .replace(/color-scheme:\s*light dark;/g, "")
  .replace(/:root(\[[^\]]+\])?/g, (_, attribute: string | undefined) => attribute ? `:host(${attribute})` : ":host");
export function PickerPicture({ density = "comfortable", icon = "ibis", appearance, split = false, headerOnly = false, rowsOnly = false }: {
  density?: Density; icon?: SearchIcon; appearance?: string; split?: boolean; headerOnly?: boolean; rowsOnly?: boolean;
}) {
  const { text } = useInterfaceMessages();
  const record: SearchResult = {
    revision: "sample", partial: false,
    id: { source: "sample", key: "vale2021" }, key: "vale2021", title: text("look-sample-title"),
    authors: text("look-sample-authors"), first_author: text("look-sample-authors"), author_count: 2,
    container: text("look-sample-journal"), year: 2021, item_type: "article-journal", matched: [],
  };
  const html = renderToStaticMarkup(<>
    {!rowsOnly && <PickerHeader inputRef={createRef()} query="" onQueryChange={ignore} onSearchKeyDown={ignore}
      expanded={false} activeDescendant={undefined} format="pandoc" showFormat={false} styles={false}
      onFormatChange={ignore} onFormatCommand={() => ({ run: ignore, isCurrent: () => false })} pinned={false} onTogglePin={ignore} icon={icon} />}
    {!headerOnly && <div className="results-area"><ul id="results">{(["look-sample-title", "look-sample-title-2", "look-sample-title-3"] as const).map((title, index) =>
      <ResultRow key={title} record={{ ...record, title: text(title) }} index={index} active={index === 0} added={false} density={density} hoverAllowed={false} library={null}
        handlers={{ onPointerDown: ignore, onClick: ignore, onContext: ignore, onTileClick: ignore }} />
    )}</ul></div>}
  </>);
  const scheme = split ? "light" : appearance === "light" || appearance === "dark" ? appearance : undefined;
  return <span className="ui-picker-picture" data-header={headerOnly || undefined} data-rows={rowsOnly || undefined} data-density={density} data-split={split || undefined} aria-hidden="true" inert>
    <Snapshot html={html} density={density} scheme={scheme} />
    {split && <Snapshot html={html} density={density} scheme="dark" dark />}
  </span>;
}

function Snapshot({ html, density, scheme, dark = false }: { html: string; density: Density; scheme?: "light" | "dark"; dark?: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!host.current) return;
    const root = host.current.shadowRoot ?? host.current.attachShadow({ mode: "open" });
    // React escapes the invented data. Only the bundled component styles and
    // brand markup are inserted; no controls, autofocus or drag regions go live.
    root.innerHTML = `<style>${styles}</style>${html.replace(/data-tauri-drag-region="[^"]*"/g, "")}`;
  }, [html]);
  return <span className="ui-picker-picture-mask" data-dark={dark || undefined} style={{ colorScheme: scheme }}>
    <span className="ui-picker-picture-content">
      <div ref={host} style={{ colorScheme: scheme }} data-platform="macos" data-density={density} inert />
    </span>
  </span>;
}
