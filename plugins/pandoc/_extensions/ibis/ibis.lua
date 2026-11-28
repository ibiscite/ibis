-- SPDX-License-Identifier: MIT
-- Ibis for Pandoc and Quarto: fills in the references a document cites from
-- the libraries Ibis reads, so citeproc formats them. It asks the `ibis`
-- command, never a library file, and adds only keys the document's own
-- `references` and `bibliography` do not already hold. Run it before citeproc:
--
--   pandoc paper.md --lua-filter ibis.lua --citeproc -o paper.docx
--
-- With no `ibis` options it reads the libraries chosen in the Ibis app. Front
-- matter can name libraries instead, as `ibis --file` and `ibis --zotero` do:
--
--   ibis:
--     file: [thesis.bib]
--     zotero: true
--
-- IBIS names the command when it is not on PATH.

local MAX_ITEMS = 1000 -- one citation's limit in `ibis cite` and `ibis search`
local MAX_QUERY = 64 * 1024 -- the longest pasted list `ibis search` reads

local function warn(message)
  io.stderr:write("[WARNING] ibis: " .. message .. "\n")
end

local function command()
  local named = os.getenv("IBIS")
  if named and named ~= "" then
    return named
  end
  return "ibis"
end

local function list(value)
  if value == nil then
    return {}
  end
  if pandoc.utils.type(value) == "List" then
    return value
  end
  return { value }
end

local function library_args(meta)
  local args = {}
  local options = meta.ibis
  if options == nil then
    return args
  end
  if pandoc.utils.type(options) ~= "table" then
    warn("ibis in the front matter must hold file or zotero; reading the app's libraries")
    return args
  end
  for _, file in ipairs(list(options.file)) do
    args[#args + 1] = "--file"
    args[#args + 1] = pandoc.utils.stringify(file)
  end
  if options.zotero == true then
    args[#args + 1] = "--zotero"
  end
  return args
end

local function append(target, items)
  for _, item in ipairs(items) do
    target[#target + 1] = item
  end
  return target
end

-- Runs `ibis ARGS`. Exit 1 means findings and still carries the answer, so its
-- output is kept; Ibis explains on stderr, which Pandoc passes on.
local function run(args, input)
  local ok, result = pcall(pandoc.pipe, command(), args, input or "")
  if ok then
    return result
  end
  if type(result) == "table" or type(result) == "userdata" then
    local code = result.error_code
    if code == 1 then
      return result.output
    end
    if code == 127 or code == nil then
      return nil, "missing"
    end
  end
  return nil, "failed"
end

-- The cited keys Ibis's libraries hold, by its exact pasted list, which
-- answers what is missing in one read.
local function held(keys, libraries)
  local query = {}
  for i, key in ipairs(keys) do
    query[i] = "@{" .. key .. "}"
  end
  local out, why = run(append({ "search", table.concat(query, " "), "--json" }, libraries))
  if out == nil then
    return nil, why
  end
  local ok, answer = pcall(pandoc.json.decode, out, false)
  if not ok or type(answer) ~= "table" or not answer.ok then
    return nil, "failed"
  end
  local list = answer.data and answer.data.list
  if list == nil then
    return keys
  end
  -- The list matches keys in any letter case; citeproc needs the exact one.
  local exact, folded = {}, {}
  for _, hit in ipairs(answer.data.hits or {}) do
    local record = hit.item and hit.item.object and hit.item.object.data or {}
    for _, key in ipairs(record.citation_keys or {}) do
      exact[key.value] = true
      folded[key.value:lower()] = key.value
    end
  end
  local found = {}
  for _, key in ipairs(keys) do
    if exact[key] then
      found[#found + 1] = key
    elseif folded[key:lower()] then
      warn("@" .. key .. " is not in your libraries; did you mean @" .. folded[key:lower()] .. "?")
    end
  end
  return found
end

-- CSL-JSON for the keys. When one key cannot be cited (a key that differs
-- only in case, or one two libraries share), the rest are cited one by one
-- and Ibis names the one it refused.
local function records(keys, libraries)
  local args = append({ "cite", "--keys-from", "-", "--format", "csl-json" }, libraries)
  local out = run(args, table.concat(keys, "\n") .. "\n")
  if out ~= nil then
    return { out }
  end
  if #keys == 1 then
    return {}
  end
  local each = {}
  for _, key in ipairs(keys) do
    append(each, records({ key }, libraries))
  end
  return each
end

-- The keys in batches `held` can ask about at once: at most MAX_ITEMS keys,
-- and MAX_QUERY bytes once written as `@{key} `.
local function batches(keys)
  local all, batch, size = {}, {}, 0
  for _, key in ipairs(keys) do
    local bytes = #key + 4
    if #batch == MAX_ITEMS or size + bytes > MAX_QUERY then
      all[#all + 1], batch, size = batch, {}, 0
    end
    batch[#batch + 1] = key
    size = size + bytes
  end
  all[#all + 1] = batch
  return all
end

local function cited(doc, known)
  local keys, seen = {}, {}
  local function add(cite)
    for _, citation in ipairs(cite.citations) do
      local id = citation.id
      -- `@*` in nocite asks for a whole bibliography, never a whole library.
      if id ~= "*" and not known[id] and not seen[id] and not id:find("[{}\n]") and id:sub(1, 1) ~= "#" then
        seen[id] = true
        keys[#keys + 1] = id
      end
    end
  end
  -- The body and the front matter: nocite, and citations in a title or abstract.
  doc:walk({ Cite = add })
  return keys
end

function Pandoc(doc)
  local known = {}
  local ok, references = pcall(pandoc.utils.references, doc)
  for _, reference in ipairs(ok and references or {}) do
    known[reference.id] = true
  end
  local keys = cited(doc, known)
  if #keys == 0 then
    return nil
  end
  local libraries = library_args(doc.meta)
  local added = pandoc.List(doc.meta.references or {})
  for _, chunk in ipairs(batches(keys)) do
    local found, why = held(chunk, libraries)
    if found == nil then
      if why == "missing" then
        warn("the ibis command was not found; install it from Ibis's Settings or set IBIS to its path")
      else
        warn("Ibis could not read the libraries; see its message above")
      end
      return nil
    end
    if #found > 0 then
      for _, json in ipairs(records(found, libraries)) do
        -- Pandoc matches a citation to a record's `id`, but the document cites
        -- the key; a library's own `id` may differ, or name another work.
        local items = pandoc.json.decode(json, false)
        for _, item in ipairs(items) do
          item.id = item["citation-key"] or item.id
        end
        added:extend(pandoc.read(pandoc.json.encode(items), "csljson").meta.references or {})
      end
    end
  end
  doc.meta.references = added
  return doc
end
