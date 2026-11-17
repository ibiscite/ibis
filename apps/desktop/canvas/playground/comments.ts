// The playground's comments, as vite.playground.config.mjs reads and writes them.
export type Where = { scenario: string; theme: string; platform: string; text: number; variant: string };
export type Comment = Where & { id: number; done: boolean; selector: string; comment: string };

const endpoint = "/__playground/comments";

export async function loadComments(): Promise<Comment[]> {
  const response = await fetch(endpoint);
  return response.ok ? response.json() : [];
}

async function post(body: object): Promise<void> {
  const response = await fetch(endpoint, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  if (!response.ok) alert(`The comment was not saved: ${(await response.json()).error}`);
}

export const saveComment = (comment: Where & { selector: string; comment: string }) => post(comment);
export const markComment = (id: number, done: boolean) => post({ id, done });
