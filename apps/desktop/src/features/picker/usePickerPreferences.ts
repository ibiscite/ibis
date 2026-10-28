// The picker's saved formatting and view preferences.
import { useEffect, useRef, useState } from "react";
import type * as nativeApi from "../../api";
import type { CopyFormat, Density, PickerPreferences } from "../../api";
import { styleOf, type Choice } from "./choice";
import type { Operation } from "./Notices";

const DENSITIES: Density[] = ["compact", "comfortable", "spacious"];
const SAVE_BEFORE_QUIT_MS = 2_000;

type PreferencesApi = Pick<typeof nativeApi, "copyFormat" | "pickerPreferences" | "setPickerPreferences" | "quit">;

export function usePickerPreferences(
  { copyFormat, pickerPreferences, setPickerPreferences, quit }: PreferencesApi,
  { styles, report, perform }: {
    /** Whether formatted citation styles can be offered. */
    styles: boolean;
    report: (operation: Operation, failed?: boolean) => void;
    perform: (operation: Operation, action: () => Promise<unknown>) => Promise<void>;
  },
) {
  const [ready, setReady] = useState(false);
  const [format, setFormat] = useState<Choice>("pandoc");
  // Saved preferences, once read; choices made before that wait in `pending`
  // so a slow read never overwrites them and a failed read never resets the
  // file to defaults.
  const [density, setDensity] = useState<Density>("comfortable");
  const [largerText, setLargerText] = useState(false);
  const [searchMark, setSearchMark] = useState(false);
  const [showPastePreview, setShowPastePreview] = useState(true);
  const previewVisibility = useRef(true);
  const savedPreferences = useRef<PickerPreferences | null>(null);
  const pending = useRef<Partial<PickerPreferences>>({});
  const pendingKeyFormat = useRef<{ format: CopyFormat; saved: boolean } | null>(null);
  const preferenceReads = useRef(0);
  const preferenceChanges = useRef(0);
  const preferenceSave = useRef<Promise<unknown>>(Promise.resolve());
  const preferenceLoad = useRef<Promise<void>>(Promise.resolve());
  const applyView = (value: PickerPreferences) => {
    setDensity(value.density);
    setLargerText(value.larger_text);
    setSearchMark(value.search_mark ?? false);
    previewVisibility.current = value.show_paste_preview;
    setShowPastePreview(value.show_paste_preview);
  };
  const savePreferences = (change: Partial<PickerPreferences>, keyFormat: CopyFormat | null = null) => {
    const revision = ++preferenceChanges.current;
    const reading = preferenceReads.current;
    if (keyFormat !== null) pendingKeyFormat.current = { format: keyFormat, saved: false };
    pending.current = { ...pending.current, ...change };
    if (!savedPreferences.current) return;
    // Send only explicit changes; a stale window never rewrites unrelated choices.
    const queued = { ...pending.current };
    const deliberate = new Set(Object.keys(change));
    const chosenKey = pendingKeyFormat.current;
    const save = preferenceSave.current.catch(() => undefined)
      .then(async () => {
        // Earlier writes may have acknowledged fields while this operation waited.
        const patch = Object.fromEntries(Object.entries(queued).filter(([key, value]) =>
          deliberate.has(key) || pending.current[key as keyof PickerPreferences] === value));
        const saved = await setPickerPreferences(patch, chosenKey && !chosenKey.saved ? chosenKey.format : null);
        return { saved, patch };
      })
      .then(({ saved, patch }) => {
        for (const key of Object.keys(patch) as (keyof PickerPreferences)[]) {
          if (pending.current[key] === patch[key]) delete pending.current[key];
        }
        if (chosenKey) chosenKey.saved = true;
        if (pendingKeyFormat.current === chosenKey) pendingKeyFormat.current = null;
        if (revision !== preferenceChanges.current || reading !== preferenceReads.current) return;
        savedPreferences.current = saved.preferences;
        applyView(saved.preferences);
        setFormat(saved.preferences.citation_style ? `style:${saved.preferences.citation_style}` : saved.copy_format);
      });
    preferenceSave.current = save;
    void perform("preferences", () => save);
  };
  const quitAfterSaving = async () => {
    // Drain choices already made, including ones awaiting the initial read.
    // A failed or stuck settings command must never trap someone in the app.
    let timer: number | undefined;
    await Promise.race([
      preferenceLoad.current.then(() => preferenceSave.current).catch(() => undefined),
      new Promise<void>((resolve) => { timer = window.setTimeout(resolve, SAVE_BEFORE_QUIT_MS); }),
    ]);
    window.clearTimeout(timer);
    await quit();
  };
  const chooseFormat = (choice: Choice) => {
    setFormat(choice);
    const chosenStyle = styleOf(choice);
    savePreferences({ citation_style: chosenStyle }, chosenStyle ? null : choice as CopyFormat);
  };
  const changeDensity = () => {
    const next = DENSITIES[(DENSITIES.indexOf(density) + 1) % DENSITIES.length] ?? "comfortable";
    setDensity(next);
    savePreferences({ density: next });
  };
  const toggleLargerText = () => {
    setLargerText(!largerText);
    savePreferences({ larger_text: !largerText });
  };
  const toggleSearchMark = () => {
    setSearchMark(!searchMark);
    savePreferences({ search_mark: !searchMark });
  };
  const togglePastePreview = () => {
    const next = !previewVisibility.current;
    previewVisibility.current = next;
    setShowPastePreview(next);
    savePreferences({ show_paste_preview: next });
  };
  useEffect(() => {
    document.documentElement.dataset.density = density;
    document.documentElement.dataset.textSize = largerText ? "larger" : "normal";
    return () => {
      delete document.documentElement.dataset.density;
      delete document.documentElement.dataset.textSize;
    };
  }, [density, largerText]);
  /** Reads the saved key format and preferences. The picker calls this from its
   * mount effect, after the library status, so the calls keep their order. */
  const loadPreferences = (mounted: () => boolean) => {
    const request = ++preferenceReads.current;
    const revision = preferenceChanges.current;
    const loadedBefore = savedPreferences.current !== null;
    // A notification cannot read behind this window's own outstanding writes.
    preferenceLoad.current = preferenceSave.current.catch(() => undefined)
      .then(() => Promise.allSettled([copyFormat(), pickerPreferences()])).then(([key, saved]) => {
      if (!mounted() || request !== preferenceReads.current || loadedBefore && revision !== preferenceChanges.current) return;
      setReady(true);
      if (key.status === "rejected") report("format");
      if (saved.status === "rejected") {
        report("preferences");
        if (pending.current.citation_style === undefined && key.status === "fulfilled") setFormat(key.value);
        return;
      }
      const loaded = { ...saved.value, ...pending.current } as PickerPreferences;
      savedPreferences.current = saved.value;
      applyView(loaded);
      const chosen = pending.current.citation_style !== undefined;
      if (Object.keys(pending.current).length > 0) savePreferences({});
      if (chosen) return;
      const style = saved.value.citation_style;
      if (style && styles) setFormat(`style:${style}`);
      else if (key.status === "fulfilled") setFormat(key.value);
    });
  };

  return { ready, format, density, largerText, searchMark, showPastePreview, chooseFormat, changeDensity, toggleLargerText, toggleSearchMark, togglePastePreview, quitAfterSaving, loadPreferences };
}
