import { COPY_FORMATS } from "../../api";
import { FORMATTED_STYLES } from "../../format/formatted";
import { pickerText } from "../../messages";
import { choiceName, type Choice } from "./choice";

/** The copy format control: key syntaxes, and citation styles when they are offered.
 * The closed control shows one word; the list and screen readers get the full name. */
export function FormatSelect({ format, styles, onChange }: {
  format: Choice;
  /** Whether formatted citation styles are offered. */
  styles: boolean;
  onChange: (choice: Choice) => void;
}) {
  return (
    <span className="format-picker" title={pickerText.copyAs}>
      <span aria-hidden="true" className="format-name">{choiceName(format)}</span>
      <label className="visually-hidden" htmlFor="format">
        {pickerText.copyAs}
      </label>
      <select id="format" value={format} onChange={(e) => onChange(e.target.value as Choice)}>
        <optgroup label={pickerText.keyFormats}>
          {COPY_FORMATS.map((f) => (
            <option key={f.value} value={f.value}>
              {f.label}
            </option>
          ))}
        </optgroup>
        {styles && (
          <optgroup label={pickerText.formattedStyles}>
            {FORMATTED_STYLES.map((s) => (
              <option key={s.id} value={`style:${s.id}`}>
                {s.title}
              </option>
            ))}
          </optgroup>
        )}
      </select>
    </span>
  );
}
