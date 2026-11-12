import { Icon } from "./ui/Icon";
export { Icon, type IconName, type IconSize } from "./ui/Icon";

/** The seven type icons; every CSL item type maps to one of them. */
export type ReferenceKind = "article" | "book" | "chapter" | "thesis" | "web" | "report" | "other";

const kindOfType: Record<string, ReferenceKind> = {
  "article": "article", "article-journal": "article", "article-magazine": "article", "article-newspaper": "article",
  "paper-conference": "article", "review": "article", "review-book": "article", "manuscript": "article", "periodical": "article",
  "book": "book", "classic": "book", "collection": "book",
  "chapter": "chapter", "entry": "chapter", "entry-dictionary": "chapter", "entry-encyclopedia": "chapter",
  "thesis": "thesis",
  "webpage": "web", "post": "web", "post-weblog": "web",
  "report": "report", "standard": "report",
};

/** The type icon for a CSL item type (as sent with each result); anything else is "other". */
export function kindOf(itemType: string | undefined): ReferenceKind {
  return itemType !== undefined && Object.hasOwn(kindOfType, itemType) ? kindOfType[itemType] ?? "other" : "other";
}

export function TypeIcon({ itemType }: { itemType: string | undefined }) {
  return <Icon name={kindOf(itemType)} size="tile" className="type-icon" />;
}

export function AddGlyph({ added }: { added: boolean }) {
  return <Icon name={added ? "check" : "plus"} size="tile" className="add-glyph" />;
}

export type ChoiceIconName = "library" | "sample" | "file" | "find" | "retry" | "swap" | "write";
export function ChoiceIcon({ name }: { name: ChoiceIconName }) {
  return <Icon name={name} size="tile" className="type-icon" />;
}

export function PageIcon({ name }: { name: "general" | "look" | "citations" | "shortcuts" | "about" }) {
  return <Icon name={name} />;
}
