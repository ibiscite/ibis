// Records the exact bytes Node's http client sends when configured as the
// VS Code "Citation Picker for Zotero" 0.2.1 client.ts requestText() does
// (commit fe4d6bbfc6): GET, a private agent without keep-alive, and these headers.
import http from "node:http";
import net from "node:net";
const TOKEN = "0123456789abcdef0123456789abcdef";
const formats = ["configured", "pandoc", "biblatex", "natbib", "typst", "formatted-citation", "formatted-bibliography"];
const server = net.createServer();
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const captures = [];
server.on("connection", (socket) => {
  let bytes = Buffer.alloc(0);
  socket.on("data", (chunk) => {
    bytes = Buffer.concat([bytes, chunk]);
    if (bytes.includes("\r\n\r\n")) { captures.push(bytes.toString("latin1")); socket.end("HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); }
  });
});
for (const format of formats) {
  // pickerURL(): the configured URL, with format replaced unless "configured".
  const url = new URL(`http://127.0.0.1:${port}/${TOKEN}/better-bibtex/cayw?format=pandoc`);
  if (format !== "configured") url.searchParams.set("format", format);
  await new Promise((resolve, reject) => {
    const agent = new http.Agent({ keepAlive: false });
    const req = http.request(url, { method: "GET", agent, headers: { "User-Agent": "vscode-zotero/0.2.0", Accept: "*/*" } });
    req.on("error", reject);
    req.on("response", (response) => { response.resume(); response.on("end", () => { agent.destroy(); resolve(); }); });
    req.end();
  });
}
server.close();
// Port-independent fixture: the recorded port is written as {port}.
process.stdout.write(JSON.stringify({ client: "Citation Picker for Zotero 0.2.1 (mblode/vscode-zotero fe4d6bbfc6)", node: process.version,
  requests: captures.map((text) => text.replaceAll(String(port), "{port}")) }, null, 1) + "\n");
