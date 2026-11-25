import { render } from "@testing-library/react";
import { expect, test } from "vitest";
import { PickerPicture } from "./PickerPicture";

test.each(["light", "dark"])("the %s snapshot owns its theme at the shadow host and renders the real list", (appearance) => {
  const { container } = render(<PickerPicture appearance={appearance} />);
  const host = container.querySelector<HTMLElement>(".ui-picker-picture-content > div")!;
  expect(host.style.colorScheme).toBe(appearance);
  expect(host.shadowRoot!.querySelector(".results-area > #results")).toBeTruthy();
});
