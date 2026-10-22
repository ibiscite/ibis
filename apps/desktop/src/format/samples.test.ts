import { Formatter, STYLES, type CslItem } from "./engine";
import { STYLE_SAMPLES, type StyleId } from "./styles";

// The format menu shows each style's in-text citation; it must be what the
// bundled style really gives for a one-author 2021 reference. Synthetic only.
const garcia: CslItem = {
  id: "garcia2021",
  type: "article-journal",
  title: "Synthetic sleep study",
  author: [{ family: "García", given: "Ana" }],
  issued: { "date-parts": [[2021]] },
  "container-title": "Journal of Tests",
};

test("each style's sample in the format menu is the style's real citation", () => {
  const formatter = new Formatter();
  for (const style of Object.keys(STYLE_SAMPLES) as StyleId[]) {
    expect(style in STYLES).toBe(true);
    expect(formatter.citation(style, [garcia], [{ id: "garcia2021" }], "text").output).toBe(STYLE_SAMPLES[style]);
  }
});
