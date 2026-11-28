// The picker contract uses named structs, string enums, internally tagged
// request enums and scalar/array properties. Fail on any other schema shape
// rather than guessing its ABI.
const camel = (name) => name.replace(/[-_]([a-zA-Z])/g, (_, letter) => letter.toUpperCase());
const identifier = /^[a-zA-Z_]\w*$/;
const keywords = new Set(["case", "default", "enum", "let", "protocol", "repeat", "self", "struct", "subscript", "typealias", "var"]);
function fieldName(wire) {
  const name = camel(wire);
  if (!identifier.test(name)) throw new Error(`invalid Swift name ${wire}`);
  return keywords.has(name) ? `\`${name}\`` : name;
}

export function swiftBindings(methods) {
  const definitions = new Map();
  function define(name, schema) {
    const previous = definitions.get(name);
    if (previous && JSON.stringify(previous) !== JSON.stringify(schema)) {
      throw new Error(`conflicting picker schemas for ${name}`);
    }
    definitions.set(name, schema);
  }

  function root(schema) {
    const { $defs, $schema, title, ...body } = schema;
    if (!identifier.test(title ?? "")) throw new Error(`picker schema needs a named root`);
    for (const [name, definition] of Object.entries($defs ?? {})) define(name, definition);
    define(title, body);
    return title;
  }

  function type(schema) {
    if (schema.$ref) {
      const name = schema.$ref.split("/").at(-1);
      if (!definitions.has(name)) throw new Error(`unknown picker schema ${name}`);
      return name;
    }
    if (Array.isArray(schema.type) && schema.type.length === 2 && schema.type.includes("null")) {
      return `${type({ ...schema, type: schema.type.find((kind) => kind !== "null") })}?`;
    }
    if (schema.anyOf?.length === 2 && schema.anyOf.some((part) => part.type === "null")) {
      return `${type(schema.anyOf.find((part) => part.type !== "null"))}?`;
    }
    switch (schema.type) {
      case "string": return "String";
      case "boolean": return "Bool";
      case "integer": return schema.minimum >= 0 ? "UInt64" : "Int64";
      case "array": return `[${type(schema.items)}]`;
      default: throw new Error(`unsupported picker property ${JSON.stringify(schema)}`);
    }
  }

  function declaration(name, schema) {
    const strings = (part) => part.type === "string" && (part.enum
      ?? (typeof part.const === "string" ? [part.const] : undefined));
    const variants = strings(schema)
      || (schema.oneOf?.every(strings) ? schema.oneOf.flatMap(strings) : undefined);
    if (variants) {
      const cases = variants.map((wire) => `    case ${fieldName(wire)} = ${JSON.stringify(wire)}`);
      return `enum ${name}: String, Codable, Hashable, Sendable {\n${cases.join("\n")}\n}`;
    }
    const tag = tagOf(schema);
    if (tag) return taggedUnion(name, schema, tag);
    if (schema.type === "object" && schema.properties) {
      const required = new Set(schema.required ?? []);
      const fields = Object.entries(schema.properties).map(([wire, value]) => {
        const name = fieldName(wire);
        const base = type(value);
        const optional = required.has(wire) || base.endsWith("?") ? "" : "?";
        return { wire, name, line: `    let ${name}: ${base}${optional}` };
      });
      const codingKeys = fields.some(({ wire, name }) => wire !== name) ? [
        "",
        "    enum CodingKeys: String, CodingKey {",
        ...fields.map(({ wire, name }) => `        case ${name} = ${JSON.stringify(wire)}`),
        "    }",
      ] : [];
      return [`struct ${name}: Codable, Hashable, Sendable {`, ...fields.map(({ line }) => line), ...codingKeys, "}"].join("\n");
    }
    return `typealias ${name} = ${type(schema)}`;
  }

  // An internally tagged enum (`#[serde(tag = "...")]`): every variant is an
  // object whose tag property is one constant string. Requests only, so encode.
  function tagOf(schema) {
    const parts = schema.oneOf;
    if (!parts?.every((part) => part.type === "object" && part.properties)) return undefined;
    const tags = Object.keys(parts[0].properties).filter((key) =>
      parts.every((part) => typeof part.properties[key]?.const === "string"));
    return tags.length === 1 ? tags[0] : undefined;
  }

  function taggedUnion(name, schema, tag) {
    const keys = new Map([[tag, fieldName(tag)]]);
    const variants = schema.oneOf.map((part) => {
      const required = new Set(part.required ?? []);
      const fields = Object.entries(part.properties).filter(([wire]) => wire !== tag).map(([wire, value]) => {
        const base = type(value);
        const optional = required.has(wire) || base.endsWith("?") ? "" : "?";
        keys.set(wire, fieldName(wire));
        return { wire, name: fieldName(wire), type: `${base}${optional}` };
      });
      return { wire: part.properties[tag].const, name: fieldName(part.properties[tag].const), fields };
    });
    const cases = variants.map(({ name, fields }) => fields.length
      ? `    case ${name}(${fields.map((field) => `${field.name}: ${field.type}`).join(", ")})`
      : `    case ${name}`);
    const encodes = variants.map(({ wire, name, fields }) => [
      fields.length ? `        case let .${name}(${fields.map((field) => field.name).join(", ")}):` : `        case .${name}:`,
      `            try container.encode(${JSON.stringify(wire)}, forKey: .${fieldName(tag)})`,
      ...fields.map((field) => field.type.endsWith("?")
        ? `            try container.encodeIfPresent(${field.name}, forKey: .${field.name})`
        : `            try container.encode(${field.name}, forKey: .${field.name})`),
    ].join("\n"));
    return [
      `enum ${name}: Encodable, Hashable, Sendable {`,
      ...cases,
      "",
      "    private enum CodingKeys: String, CodingKey {",
      ...[...keys].map(([wire, key]) => `        case ${key} = ${JSON.stringify(wire)}`),
      "    }",
      "",
      "    func encode(to encoder: Encoder) throws {",
      "        var container = encoder.container(keyedBy: CodingKeys.self)",
      "        switch self {",
      ...encodes,
      "        }",
      "    }",
      "}",
    ].join("\n");
  }

  const calls = methods.map(({ method, params, result }) => {
    const request = root(params);
    const reply = root(result);
    // Named for the method, so a request type can be named for what it holds.
    const name = `${method.split("/").map((part) => camel(part[0].toUpperCase() + part.slice(1))).join("")}Method`;
    return [
      `enum ${name}: PickerRPC {`,
      `    static let name = ${JSON.stringify(method)}`,
      `    typealias Parameters = ${request}`,
      `    typealias Reply = ${reply}`,
      "}",
    ].join("\n");
  });
  return [
    "// Generated by node scripts/gen.mjs from the engine's picker method schemas. Do not edit.",
    "protocol PickerRPC {\n    static var name: String { get }\n    associatedtype Parameters: Encodable\n    associatedtype Reply: Decodable\n}",
    ...calls,
    ...[...definitions.entries()].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
      .map(([name, schema]) => declaration(name, schema)),
    "",
  ].join("\n\n");
}

export function swiftWords(languages) {
  const labels = Object.keys(languages["en-US"]);
  const dictionaries = Object.entries(languages).map(([language, words]) => {
    if (Object.keys(words).join() !== labels.join()) throw new Error(`picker words differ for ${language}`);
    if (Object.values(words).some((value) => !value || /^picker-/.test(value))) {
      throw new Error(`missing picker words in ${language}`);
    }
    return [
      `        .${fieldName(language)}: [`,
      ...labels.map((word) => `            .${fieldName(word)}: ${JSON.stringify(words[word])},`),
      "        ],",
    ].join("\n");
  });
  return [
    "// Generated by node scripts/gen.mjs from the Fluent catalogue. Do not edit.",
    "import Foundation",
    "",
    "enum PickerWord {",
    ...labels.map((word) => `    case ${fieldName(word)}`),
    "}",
    "",
    "enum PickerWords {",
    "    static let language: Language = Locale.preferredLanguages.lazy.compactMap { tag in",
    '        let primary = tag.split(whereSeparator: { "-_".contains($0) }).first?.lowercased()',
    '        return ["en": .enUS, "de": .de, "ar": .ar][primary ?? ""]',
    "    }.first ?? .enUS",
    "",
    "    static func text(_ key: PickerWord, _ value: String? = nil) -> String {",
    '        let template = words[language]?[key] ?? ""',
    '        return value.map { template.replacingOccurrences(of: "{IBIS_VALUE}", with: $0) } ?? template',
    "    }",
    "",
    "    private static let words: [Language: [PickerWord: String]] = [",
    ...dictionaries,
    "    ]",
    "}",
    "",
  ].join("\n");
}
