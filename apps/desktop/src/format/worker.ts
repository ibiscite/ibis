// Runs the citation engine off the interface thread. The client stops this
// worker if a request takes too long (see client.ts).
import { Formatter } from "./engine";
import type { FormatReply, FormatRequest } from "./client";

const formatter = new Formatter();

self.onmessage = (event: MessageEvent<FormatRequest>) => {
  const request = event.data;
  let reply: FormatReply;
  try {
    const result = request.kind === "citation"
      ? formatter.citation(request.style, request.items, request.cites, request.format, request.segments)
      : formatter.bibliography(request.style, request.items, request.format);
    reply = { id: request.id, result };
  } catch {
    reply = { id: request.id, error: "the citation could not be formatted" };
  }
  self.postMessage(reply);
};
