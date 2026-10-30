import { createRef, useLayoutEffect, useRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import type { Density, SearchResult } from "../bindings";
import { PickerHeader } from "../features/picker/PickerHeader";
import { ResultRow } from "../features/picker/ResultList";
import { useInterfaceMessages } from "../i18n";
import pickerStyles from "../styles.css?raw";

const ignore = () => undefined;
// Shadow isolation reuses the current picker CSS without affecting Settings or
// introducing duplicate document IDs, live controls, autofocus or drag regions.
const styles = pickerStyles.replace(/@import[^;]+;/g, "").replace(/:root(\[[^\]]+\])?/g, (_, attribute: string | undefined) => attribute ? `:host(${attribute})` : ":host");
export function PickerPicture({ density = "comfortable", icon = "ibis", appearance, split = false, headerOnly = false }: {
  density?: Density; icon?: "ibis" | "magnifier"; appearance?: string; split?: boolean; headerOnly?: boolean;
}) {
  const { text } = useInterfaceMessages();
  const record: SearchResult = {
    revision: "sample", partial: false,
    id: { source: "sample", key: "vale2021" }, key: "vale2021", title: text("look-sample-title"),
    authors: text("look-sample-authors"), first_author: text("look-sample-authors"), author_count: 2,
    container: text("look-sample-journal"), year: 2021, item_type: "article-journal", matched: [],
  };
  const html = renderToStaticMarkup(<>
    <PickerHeader inputRef={createRef()} query="" onQueryChange={ignore} onSearchKeyDown={ignore}
      expanded={false} activeDescendant={undefined} format="pandoc" showFormat={false} styles={false}
      onFormatChange={ignore} onFormatCommand={() => ({ run: ignore, isCurrent: () => false })} pinned={false} onTogglePin={ignore} mark={icon === "ibis"} />
    {!headerOnly && <ul><ResultRow record={record} index={0} active added={false} density={density} hoverAllowed={false} library={null}
      handlers={{ onPointerDown: ignore, onClick: ignore, onContext: ignore, onTileClick: ignore }} />
      <ResultRow record={record} index={1} active={false} added={false} density={density} hoverAllowed={false} library={null}
        handlers={{ onPointerDown: ignore, onClick: ignore, onContext: ignore, onTileClick: ignore }} /></ul>}
  </>);
  return <span className="ui-picker-picture" data-header={headerOnly || undefined} data-density={density} data-appearance={appearance} data-split={split || undefined} aria-hidden="true" inert>
    <Snapshot html={html} density={density} />
    {split && <Snapshot html={html} density={density} dark />}
  </span>;
}

function Snapshot({ html, density, dark = false }: { html: string; density: Density; dark?: boolean }) {
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!host.current) return;
    const root = host.current.shadowRoot ?? host.current.attachShadow({ mode: "open" });
    // React renders the invented data. The stylesheet is the bundled picker CSS.
    root.innerHTML = `<style>${styles}</style>${html.replace(/data-tauri-drag-region="[^"]*"/g, "")}`;
  }, [html]);
  return <span className="ui-picker-picture-content" data-dark={dark || undefined}>
    <div ref={host} data-platform="macos" data-density={density} inert />
  </span>;
}
