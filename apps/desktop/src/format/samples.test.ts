import { FormatClient } from "./client";
import { formatCitation } from "./formatted";
import { InlineWorker } from "./inline-worker";
import { STYLE_SAMPLES, type StyleId } from "./styles";

// The format menu shows what each style copies; it must be what the bundled
// style really gives for a one-author 2021 reference. Synthetic only.
const garcia = JSON.stringify([{
  id: "garcia2021",
  type: "article-journal",
  title: "Synthetic sleep study",
  author: [{ family: "García", given: "Ana" }],
  issued: { "date-parts": [[2021]] },
  "container-title": "Journal of Tests",
}]);

test("each style's sample in the format menu is what the style really copies", async () => {
  const client = new FormatClient(() => new InlineWorker());
  for (const style of Object.keys(STYLE_SAMPLES) as StyleId[]) {
    expect((await formatCitation(garcia, style, { client })).text).toBe(STYLE_SAMPLES[style]);
  }
});
