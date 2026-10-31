import sqlite3, sys
c = sqlite3.connect("data/zotero.sqlite")
print("version", c.execute("select version from version where schema='userdata'").fetchone())
tables = [r[0] for r in c.execute("select name from sqlite_schema where type='table' and name not like 'sqlite_%'")]
print(len(tables), "tables; fulltext-like:", [t for t in tables if "ull" in t.lower()])
print("attached?", c.execute("pragma database_list").fetchall())
print("items", c.execute("select count(*) from items").fetchone(), "itemTypes", c.execute("select typeName,count(*) from items join itemTypes using(itemTypeID) group by 1").fetchall())
A = "23456789ABCDEFGHIJKLMNPQRSTUVWXYZ"
rows = c.execute("""select i.itemID, v.value from items i
  join itemData d on d.itemID=i.itemID join fields f on f.fieldID=d.fieldID and f.fieldName='DOI'
  join itemDataValues v on v.valueID=d.valueID""").fetchall()
print("with doi", len(rows))
for item, doi in rows:
    n = int(doi.rsplit(".", 1)[1])
    s = ""
    m = n
    for _ in range(7):
        s = A[m % len(A)] + s
        m //= len(A)
    c.execute("update items set key=? where itemID=?", ("S" + s, item))
c.commit()
print(c.execute("select key from items limit 3").fetchall())
c.execute("pragma wal_checkpoint(truncate)")
c.execute("pragma journal_mode=delete")
c.close()
c = sqlite3.connect("data/fulltext.sqlite")
print("fulltext.sqlite tables", [r[0] for r in c.execute("select name from sqlite_schema where type='table'")])
