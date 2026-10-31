"""Deterministic invented citation corpus; no profile discovery or real Zotero use."""

import hashlib
import json
from pathlib import Path
import random
import shutil
import sqlite3


ROOT = Path(__file__).resolve().parents[4]
ASSUMPTIONS = {
    "abstracts": "exactly 75%; seeded uniform 80..250 words; short invented scientific vocabulary",
    "authors": "94% uniform 1..6; 5% uniform 7..30; 1% uniform 31..200; unique names per record",
    "languages": "10% CJK (Chinese/Japanese/Korean in rotation), otherwise Latin with diacritics",
    "fields": "journal/date/volume/issue/pages/DOI on all; URL 25%, keywords 33%, note/Extra 20%",
    "children": "one attachment per citation, one note per three citations; no attachment files",
    "sqlite": "userdata 130; physical closed-test schema extended with primary/foreign-key indexes, child and fulltext tables; one personal library, no groups, trash, WAL or BBT",
    "fulltext": "600,000,000 bytes assumed total DB size; invented distinct word dictionary and item-word postings with two indexes, not a large blob or sparse padding",
    "thin_benchmark": "refresh_budget.rs approximately 210 bytes/record; two authors, no abstracts or children",
}


def key(seed, index):
    return f"sp{seed}r{index}"


def native(index):
    return f"S{index:07d}"


def records(count, seed):
    rng = random.Random(seed)
    words = "star maps wave flow light dust field data model phase spin orbit cell time space mass".split()
    for i in range(count):
        cjk = i % 10 == 0
        title = f"Invented signals seed{seed} orbit {i}"
        if cjk:
            title += " " + ["星図 天文", "星図 光の観測", "星図 별의 지도"][i // 10 % 3]
        else:
            title += " " + ["Lumière", "Überblick", "Órbita", "Ångström"][i % 4]
        tier = i % 100
        authors = rng.randint(31, 200) if tier == 0 else rng.randint(7, 30) if tier < 6 else rng.randint(1, 6)
        names = []
        for j in range(authors):
            family, given = (["林", "佐藤", "김"][i // 10 % 3], ["明", "葵", "민"][i // 10 % 3]) if cjk else (["Müller", "García", "Sørensen", "Dvořák"][j % 4], ["Zoé", "René", "Áda"][j % 3])
            names.append({"creatorType": "author", "firstName": given, "lastName": f"{family}{i}x{j}"})
        fields = {"title": title, "date": str(1950 + i % 76), "publicationTitle": "Invented Letters",
                  "volume": str(i % 42 + 1), "issue": str(i % 12 + 1), "pages": f"{i % 200 + 1}-{i % 200 + 12}",
                  "DOI": f"10.1000/spike{seed}.{i}", "citationKey": key(seed, i)}
        word_count = 0
        if i % 4 != 3:
            word_count = rng.randint(80, 250)
            fields["abstractNote"] = " ".join(rng.choice(words) for _ in range(word_count)) + "."
        if i % 4 == 0:
            fields["url"] = f"https://example.org/invented/{seed}/{i}"
        if i % 5 == 0:
            fields["extra"] = f"Invented observation {i} of stellar phase."
        yield i, fields, names, word_count


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def generate(directory, count, seed, fulltext=False):
    if shutil.disk_usage(directory.parent).free < 10_000_000_000:
        raise RuntimeError("less than 10 GB free; refusing fixture generation")
    directory.mkdir()
    lean = directory / "lean"
    lean.mkdir()
    db = sqlite3.connect(lean / "zotero.sqlite")
    schema = (ROOT / "crates/ibis-zotero-local/fixtures/closed/schema.sql").read_text().split("INSERT INTO libraries")[0]
    db.executescript(schema + """
        CREATE UNIQUE INDEX itemData_pk ON itemData(itemID,fieldID);
        CREATE UNIQUE INDEX itemCreators_pk ON itemCreators(itemID,orderIndex);
        CREATE UNIQUE INDEX deletedItems_pk ON deletedItems(itemID);
        CREATE UNIQUE INDEX items_library_key ON items(libraryID,key);
        CREATE TABLE settings(setting TEXT,key TEXT,value TEXT,PRIMARY KEY(setting,key));
        CREATE TABLE itemAttachments(itemID INTEGER PRIMARY KEY,parentItemID INTEGER,linkMode INTEGER,contentType TEXT,path TEXT);
        CREATE INDEX attachments_parent ON itemAttachments(parentItemID);
        CREATE TABLE itemNotes(itemID INTEGER PRIMARY KEY,parentItemID INTEGER,note TEXT,title TEXT);
        CREATE INDEX notes_parent ON itemNotes(parentItemID);
        CREATE TABLE tags(tagID INTEGER PRIMARY KEY,name TEXT UNIQUE);
        CREATE TABLE itemTags(itemID INTEGER,tagID INTEGER,type INTEGER,PRIMARY KEY(itemID,tagID));
        CREATE TABLE fulltextWords(wordID INTEGER PRIMARY KEY,word TEXT UNIQUE);
        CREATE TABLE fulltextItemWords(wordID INTEGER,itemID INTEGER,PRIMARY KEY(wordID,itemID));
        CREATE INDEX fulltextItemWords_itemID ON fulltextItemWords(itemID);
        CREATE TABLE fulltextItems(itemID INTEGER PRIMARY KEY,indexedPages INTEGER,totalPages INTEGER,indexedChars INTEGER,totalChars INTEGER,version INTEGER,synced INTEGER);
        INSERT INTO version VALUES('userdata',130);
        INSERT INTO settings VALUES('client','lastVersion','10.0.6');
        INSERT INTO libraries VALUES(1,'user');
        INSERT INTO itemTypesCombined VALUES(2,'journalArticle'),(3,'note'),(4,'attachment');
        INSERT INTO creatorTypes VALUES(1,'author');
        PRAGMA journal_mode=DELETE;
        """)
    field_names = ["title", "date", "publicationTitle", "volume", "issue", "pages", "DOI", "citationKey", "abstractNote", "url", "extra"]
    field_ids = {name: i + 1 for i, name in enumerate(field_names)}
    db.executemany("INSERT INTO fieldsCombined VALUES(?,?)", [(i, name) for name, i in field_ids.items()])
    db.executemany("INSERT INTO tags VALUES(?,?)", [(i, f"invented-topic-{i}") for i in range(1, 8)])
    value_id, creator_id = 0, 0
    stats = {"abstract_records": 0, "abstract_words": 0, "authors": 0, "authors_1_6": 0, "authors_7_30": 0, "authors_31_200": 0, "cjk_records": 0}
    mapping = {"date": "year", "publicationTitle": "journal", "issue": "number", "DOI": "doi", "abstractNote": "abstract", "extra": "note"}
    with (directory / "library.bib").open("w") as bib, (directory / "api.jsonl").open("w") as api:
        for i, fields, names, word_count in records(count, seed):
            item_id = i + 1
            stats["authors"] += len(names)
            stats["authors_31_200" if len(names) >= 31 else "authors_7_30" if len(names) >= 7 else "authors_1_6"] += 1
            stats["abstract_records"] += bool(word_count)
            stats["abstract_words"] += word_count
            stats["cjk_records"] += i % 10 == 0
            values = [(mapping.get(name, name), value) for name, value in fields.items() if name != "citationKey"]
            values.append(("author", " and ".join(f"{n['lastName']}, {n['firstName']}" for n in names)))
            if i % 3 == 0:
                values.append(("keywords", f"invented-topic-{i % 7 + 1}"))
                db.execute("INSERT INTO itemTags VALUES(?,?,0)", (item_id, i % 7 + 1))
            bib.write("@article{" + key(seed, i) + "," + ",".join(name + "={" + value + "}" for name, value in values) + "}\n")
            data = dict(fields, key=native(i), version=1, itemType="journalArticle", creators=names,
                        dateModified="2026-01-01T00:00:00Z", tags=[{"tag": f"invented-topic-{i % 7 + 1}"}] if i % 3 == 0 else [])
            api.write(json.dumps({"key": native(i), "version": 1, "data": data}, ensure_ascii=False, separators=(",", ":")) + "\n")
            def item(identifier, item_key, kind):
                db.execute("INSERT INTO items VALUES(?,1,?,1,?,'2026-01-01 00:00:00','2026-01-01 00:00:00')", (identifier, item_key, kind))
            item(item_id, native(i), 2)
            attachment = count + item_id
            item(attachment, f"A{i:07d}", 4)
            db.execute("INSERT INTO itemAttachments VALUES(?,?,0,'application/pdf',?)", (attachment, item_id, f"storage:invented-{i}.pdf"))
            if i % 3 == 0:
                note = count * 2 + item_id
                item(note, f"N{i:07d}", 3)
                db.execute("INSERT INTO itemNotes VALUES(?,?,?,?)", (note, item_id, f"<p>Invented note on phase {i}; no real document.</p>", "Invented note"))
            for name, value in fields.items():
                db.execute("INSERT OR IGNORE INTO itemDataValues(valueID,value) VALUES(?,?)", (value_id + 1, value))
                actual = db.execute("SELECT valueID FROM itemDataValues WHERE value=?", (value,)).fetchone()[0]
                value_id += 1
                db.execute("INSERT INTO itemData VALUES(?,?,?)", (item_id, field_ids[name], actual))
            for position, name in enumerate(names):
                creator_id += 1
                db.execute("INSERT INTO creators VALUES(?,?,?,0)", (creator_id, name["firstName"], name["lastName"]))
                db.execute("INSERT INTO itemCreators VALUES(?,?,1,?)", (item_id, creator_id, position))
    db.commit()
    db.close()
    if fulltext:
        padded = directory / "fulltext"
        padded.mkdir()
        shutil.copyfile(lean / "zotero.sqlite", padded / "zotero.sqlite")
        connection = sqlite3.connect(padded / "zotero.sqlite")
        connection.execute("PRAGMA journal_mode=OFF")  # generated offline; closed reader still sees a clean DB
        connection.executemany("INSERT INTO fulltextWords VALUES(?,?)", ((i, f"inventedword{i:06d}") for i in range(1, 100001)))
        connection.execute("INSERT INTO fulltextItems SELECT itemID,20,20,80000,80000,1,0 FROM itemAttachments")
        connection.execute("CREATE TEMP TABLE offsets(n INTEGER PRIMARY KEY)")
        connection.executemany("INSERT INTO offsets VALUES(?)", [(i,) for i in range(100)])
        layer = 0
        while (padded / "zotero.sqlite").stat().st_size < 600_000_000:
            if layer >= 1000:
                raise RuntimeError("fulltext dictionary exhausted")
            connection.execute("INSERT INTO fulltextItemWords SELECT ? + n + 1,itemID FROM offsets CROSS JOIN itemAttachments", (layer * 100,))
            connection.commit()
            layer += 1
        stats["fulltext_words_per_attachment"] = layer * 100
        stats["fulltext_postings"] = layer * 100 * count
        connection.execute("PRAGMA journal_mode=DELETE")
        connection.close()
    files = [directory / "library.bib", directory / "api.jsonl", lean / "zotero.sqlite"]
    if fulltext:
        files.append(directory / "fulltext/zotero.sqlite")
    manifest = {"format": 1, "seed": seed, "records": count, "assumptions": ASSUMPTIONS, "statistics": stats,
                "files": {str(path.relative_to(directory)): {"bytes": path.stat().st_size, "mean_bytes_per_citation": path.stat().st_size / count, "sha256": digest(path)} for path in files}}
    (directory / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--records", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--fulltext", action="store_true")
    args = parser.parse_args()
    generate(args.directory, args.records, args.seed, args.fulltext)
