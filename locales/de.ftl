# Ibis’s words, for the command line, agents (MCP) and the app (#1675).
# The engine returns codes; each code has one sentence here, which every front
# end shows. A `.cli`, `.agent` or `.app` attribute says what to do next in that
# front end. Ids come from the codes’ wire names, and a test fails on a code
# without words or words without a code. People’s words, not the engine’s
# (design principle 12), and typographic apostrophes.

-library-formats = BibTeX, BibLaTeX, CSL-JSON, RIS oder CSV

## Engine errors: what went wrong, by code.

error-usage = Der Befehl ist ungültig.
    .cli = Siehe: ibis --help
    .agent = Die Argumente passen nicht zum Eingabeschema dieses Tools; prüfen Sie, welche Namen, Typen, Grenzen und Kombinationen es erlaubt.
error-bad-input = Die Eingabe ist ungültig.
    .cli = Siehe: ibis --help
    .agent = Die Argumente passen nicht zum Eingabeschema dieses Tools; prüfen Sie, welche Namen, Typen, Grenzen und Kombinationen es erlaubt.
error-not-found = Keine Treffer.
    .agent = Verwenden Sie eine ref, die search zurückgegeben hat.
error-source-unavailable = Eine Bibliothek konnte gerade nicht gelesen werden.
    .cli = Versuchen Sie es erneut.
    .agent = Versuchen Sie es gleich noch einmal.
    .app = Versuchen Sie es erneut.
error-resource-limit = Die Anfrage oder ihr Ergebnis ist zu groß.
    .cli = Fragen Sie nach weniger.
    .agent = Fragen Sie nach weniger Ergebnissen.
error-newer-format = Eine neuere Version von Ibis hat diese Daten geschrieben. Aktualisieren Sie Ibis.
error-internal = Ibis konnte dies nicht abschließen.
    .cli = Bitte melden Sie es unter https://github.com/ibiscite/ibis/issues
    .agent = Versuchen Sie es erneut.
    .app = Versuchen Sie es erneut.
error-cancelled = Die Anfrage wurde abgebrochen.

## Why, when the code alone cannot say.

reason-unknown-style = Es gibt keinen Stil mit diesem Namen.
    .cli = Die Stile auflisten mit: ibis styles
    .agent = Das Tool styles listet sie auf.
reason-unknown-output = Es gibt keine Ausgabe mit dieser ID.
    .cli = Auflisten mit: ibis outputs
    .agent = Verwenden Sie ein Format aus dem Eingabeschema dieses Tools.
reason-unknown-locale = Diese Zitiersprache ist nicht verfügbar.
    .cli = Auflisten mit: ibis styles --locales
    .agent = Das Tool styles listet die Sprachen auf.
reason-duplicate-key = Die Bibliothek vergibt diesen Zitierschlüssel an mehr als einen Eintrag, daher überspringt Ibis sie alle. Geben Sie jedem Eintrag einen eigenen Schlüssel.
reason-damaged-entry = Die Bibliothek hat einen Eintrag mit diesem Zitierschlüssel, aber Ibis konnte ihn nicht lesen und überspringt ihn daher.
    .cli = Korrigieren Sie den Eintrag; ibis libraries nennt Zeile und Grund.
    .agent = Sagen Sie der Person, dass der Eintrag mit diesem Schlüssel beschädigt ist und in ihrer Bibliothek korrigiert werden muss.
    .app = Korrigieren Sie den Eintrag in Ihrer Bibliothek.
reason-ambiguous-key = Mehrere Referenzen verwenden diesen Zitierschlüssel.
    .cli = Geben Sie jeder in Ihrer Bibliothek einen eigenen Schlüssel.
    .agent = Verwenden Sie die ref, die search zurückgegeben hat.
    .app = Geben Sie jeder in Ihrer Bibliothek einen eigenen Schlüssel.
reason-missing-key = Diese Referenz hat keinen Zitierschlüssel.
    .cli = Vergeben Sie einen in Ihrer Bibliothek, oder verwenden Sie --style.
    .agent = Verwenden Sie einen Stil, oder bitten Sie die Person, einen Schlüssel zu vergeben.
    .app = Wählen Sie einen Zitierstil.
reason-missing-file = Die Bibliotheksdatei wurde verschoben, umbenannt oder entfernt.
    .cli = Wählen Sie ihren aktuellen Ort mit --file.
    .agent = Bitten Sie die Person, die Bibliotheksdatei zu suchen, und starten Sie dann eine neue Sitzung.
    .app = Suchen Sie die Bibliotheksdatei.
reason-app-missing = Die Ibis-App wurde nicht gefunden.
    .cli = Installieren Sie sie, oder geben Sie ihren Ort mit --app <path> an.
reason-not-ibis = Das als Ibis-App angegebene Programm ist nicht Ibis.
    .cli = Geben Sie den Ort der Ibis-App mit --app <path> an.
reason-no-answer = Die Ibis-App wurde gestartet, antwortet aber nicht.
    .cli = Öffnen Sie Ibis einmal von Hand und versuchen Sie es dann erneut.
reason-settings-read-timeout = Das Lesen der Bibliothekseinstellungen der Ibis-App hat zu lange gedauert.
reason-zotero-sharing-off = Zotero ist geöffnet, gibt Ihre Bibliothek aber nicht für andere Apps frei.
    .cli = Öffnen Sie in Zotero die Einstellungen, dann Erweitert, und aktivieren Sie „Anderen Anwendungen auf diesem Computer erlauben, mit Zotero zu kommunizieren“.
    .agent = Bitten Sie die Person, in den erweiterten Einstellungen von Zotero „Anderen Anwendungen erlauben“ zu aktivieren.
reason-zotero-profile-needed = Zotero ist geschlossen, und Ibis muss wissen, welches seiner Profile es lesen soll.
    .cli = Öffnen Sie Zotero, oder nennen Sie eines mit --zotero-profile NAME.
    .agent = Bitten Sie die Person, Zotero zu öffnen oder in Ibis ein Profil zu wählen, oder übergeben Sie --zotero-profile an ibis mcp.
    .app = Öffnen Sie Zotero, oder wählen Sie ein Profil.
reason-zotero-needs-opening = Ihre Zotero-Bibliothek kann nicht gelesen werden, solange Zotero geschlossen ist.
    .cli = Öffnen Sie Zotero und versuchen Sie es dann erneut.
    .agent = Bitten Sie die Person, Zotero zu öffnen.
reason-zotero-not-found = Auf diesem Computer wurde keine Zotero-Bibliothek gefunden.
    .cli = Prüfen Sie, ob Zotero installiert ist und einmal geöffnet wurde, oder verwenden Sie --file.
reason-zotero-no-space = Es ist nicht genug Speicherplatz frei, um Zotero bei geschlossener App zu lesen.
    .cli = Geben Sie Speicherplatz frei, oder öffnen Sie Zotero.
    .app = Geben Sie Speicherplatz frei, oder öffnen Sie Zotero.
reason-csv-template = Die erste Zeile dieser CSV-Datei passt nicht zur Vorlage von Ibis: Sie muss eine Spalte für Typ und Titel nennen, und nur Spalten, die die Vorlage aufführt.
    .cli = Benennen Sie die Spalten in der ersten Zeile um; die CSV-Vorlage im Kompatibilitätsleitfaden von Ibis führt sie auf.
    .agent = Bitten Sie die Person, die Spalten in der ersten Zeile der Datei nach der Vorlage umzubenennen.
    .app = Benennen Sie die Spalten in der ersten Zeile um; die CSV-Vorlage im Kompatibilitätsleitfaden von Ibis führt sie auf.
reason-library-cut-short = In dem, was Ibis gelesen hat, hat keine Referenz diesen Schlüssel, aber die Bibliothek ist zu groß, um sie ganz zu lesen (Ibis stoppt bei 200.000 Einträgen). Der Schlüssel kann also im ungelesenen Teil stehen.
    .cli = Teilen Sie die Bibliothek in kleinere Dateien auf und geben Sie jede mit --file an.
    .agent = Bitten Sie die Person, die Bibliothek in kleinere Dateien aufzuteilen, damit Ibis die fehlenden Referenzen lesen kann.
    .app = Teilen Sie die Bibliothek in kleinere Dateien auf.
reason-unknown-format = Diese Datei hat kein Format, das Ibis liest: { -library-formats }.
    .cli = Wählen Sie mit --file eine andere Datei.
    .agent = Bitten Sie die Person, eine andere Datei zu wählen.
    .app = Wählen Sie eine andere Datei.
reason-duplicate-record = Zwei mit der Anfrage gesendete Datensätze haben dieselbe ID (1 und "1" zählen als eine), daher kann Ibis nicht erkennen, welcher gemeint ist.
    .cli = Geben Sie jedem Datensatz eine eigene ID.
    .agent = Geben Sie jedem Datensatz eine eigene ID.
reason-rejected-record = Ein mit der Anfrage gesendeter Datensatz ist kein gültiges CSL-JSON, daher zitiert Ibis keinen, statt ihn wegzulassen.
    .cli = Jeder Datensatz braucht eine id und einen CSL-Typ; korrigieren oder entfernen Sie ihn.
    .agent = Jeder Datensatz braucht eine id und einen CSL-Typ wie book oder article-journal; korrigieren oder entfernen Sie den ungültigen.
reason-no-attachment-text = Ibis kann den Text dieses Anhangs nicht lesen: Zotero hat ihn nicht indiziert, er ist ein Link, oder seine Datei fehlt, ist beschädigt oder für Ibis nicht lesbar.
    .cli = Führen Sie ibis outline für seine Referenz aus, um den Grund zu sehen; in Zotero fügt „Eintrag neu indizieren“ den Text hinzu.
    .agent = Sagen Sie es der Person; sie kann den Text in Zotero mit „Eintrag neu indizieren“ hinzufügen. Raten Sie nicht, was darin steht.
reason-attachment-outside-library = Diese Anhangsdatei liegt außerhalb des Bibliotheksordners. Sie können sie unter Einstellungen › KI-Apps › Anhangstext erlauben.
reason-no-such-pages = Dieser Anhang hat keinen Text für diese Seiten, oder es ist nicht bekannt, wo seine Seiten beginnen.
    .cli = Führen Sie ibis outline für seine Referenz aus, um zu sehen, welche Seiten Text haben, oder lesen Sie ohne --pages.
    .agent = Verwenden Sie die Seiten, die outline angibt, oder lesen Sie mit offset, wenn die Seiten nicht bekannt sind.
reason-destination-exists = Dort gibt es schon etwas mit diesem Namen. Wählen Sie einen anderen Namen oder Ort; Ibis ersetzt es nie.
reason-destination-unwritable = Ibis konnte dort nicht schreiben. Wählen Sie einen vorhandenen Ordner, in den Sie schreiben dürfen und auf dessen Laufwerk noch Platz ist.
reason-source-not-found = Ibis konnte keinen Quellenadapter mit diesem Namen starten.
    .cli = Prüfen Sie den Namen nach --source und den absoluten Pfad der ausführbaren Datei in source-adapters.json. Führen Sie ibis doctor aus, um die eingerichteten Adapter zu sehen.
    .agent = Bitten Sie die Person, den Namen der Quelle und ihre Einrichtung in ibis doctor zu prüfen.
reason-source-manager-closed = Die Literaturverwaltung ist nicht geöffnet, und diese Quelle liest sie nur, solange sie geöffnet ist.
    .cli = Öffnen Sie die Literaturverwaltung und ihre Bibliothek und versuchen Sie es dann erneut.
    .agent = Bitten Sie die Person, die Literaturverwaltung und ihre Bibliothek zu öffnen.
reason-source-library-needed = In der Literaturverwaltung sind mehrere Bibliotheken geöffnet, daher muss Ibis wissen, welche es lesen soll.
    .cli = Nennen Sie eine nach der Quelle, etwa --source endnote=NAME.
    .agent = Fragen Sie die Person, welche Bibliothek gelesen werden soll.
reason-source-library-not-found = Keine der geöffneten Bibliotheken der Literaturverwaltung hat diesen Namen.
    .cli = Öffnen Sie die Bibliothek in der Literaturverwaltung, oder prüfen Sie ihren Namen nach --source.
    .agent = Bitten Sie die Person, die Bibliothek in der Literaturverwaltung zu öffnen.
reason-source-permission = Das Betriebssystem hat Ibis nicht erlaubt, die Literaturverwaltung nach ihrer Bibliothek zu fragen.
    .cli = Erlauben Sie es unter macOS in den Systemeinstellungen, Datenschutz & Sicherheit, Automation, und versuchen Sie es dann erneut.
    .agent = Bitten Sie die Person, Ibis in den Datenschutzeinstellungen ihres Systems zu erlauben, die Literaturverwaltung zu steuern.
reason-source-failed = Der Quellenadapter hat die Bibliothek nicht gelesen: Er konnte es nicht, wurde beendet, brauchte zu lange oder antwortete so, wie Ibis es nicht akzeptiert.
    .cli = Versuchen Sie es erneut; ibis sources check zeigt, was der Adapter antwortet.
    .agent = Bitten Sie die Person, ibis sources check für diese Quelle auszuführen.
app-share-page-title = Leseliste
app-share-page-import = References.bib lädt diese Referenzen in Zotero, EndNote oder jede andere Literaturverwaltung.
app-share-page-pdfs = Der Ordner PDFs enthält die mitgelieferten PDFs.

## What a citation could not say in full.

warning-source-field = Ein Feld von { $reference } konnte nicht vollständig umgewandelt werden. Prüfen Sie es in Ihrer Bibliothek.
warning-sentence-case-expected = APA erwartet für den Titel von { $reference } Satzschreibung. Die Großschreibung wurde wie gespeichert übernommen; prüfen Sie sie in Ihrem Dokument.
loss-locator = die Seite oder Fundstelle
loss-prefix = den Text davor
loss-suffix = den Text danach
loss-author-suppression = den ausgelassenen Autor
loss-rich-text = die Textformatierung
loss-order = die Reihenfolge der Referenzen
notice-needs-document-context = Dieser Stil nummeriert Zitationen oder setzt Fußnoten über das ganze Dokument, das Ibis nicht sieht. Prüfen Sie das Ergebnis in Ihrem Dokument.
notice-reference-entry = Dies ist der vollständige Eintrag der Referenz.
notice-full-first-note = Dies ist die Vollform einer ersten Fußnote; spätere Fußnoten zum selben Werk sind meist kürzer.
notice-year-suffix-withheld = Zwei dieser Werke haben denselben Autor und dasselbe Jahr. Ergänzen Sie die Buchstaben am Jahr (etwa 2026a und 2026b) in Ihrem Dokument, denn sie hängen von seinem ganzen Literaturverzeichnis ab.
notice-input-language-unsupported = Ibis erkennt Fundstellen-Wörter in dieser Sprache nicht. Geben Sie die Seite oder Stelle auf Englisch an, etwa „p. 12“.

## Citation forms, by their one name.

form-normal = Name in Klammern
form-narrative = Name im Satz
form-author-only = Nur Name
form-year-only = Nur Jahr
form-full = Vollständige Referenz
form-bibliography-only = Literaturverzeichnis

## Why a library skipped a record or asks for a field to be checked. `.field`
## is used when the source names the field.

cause-not-closed = er wird nie geschlossen
cause-no-key = er hat keinen Zitierschlüssel
cause-missing-value = ein Feld hat keinen Wert
    .field = { $field } hat keinen Wert
cause-unbalanced = die Klammern oder Anführungszeichen eines Felds sind nicht paarig
    .field = die Klammern oder Anführungszeichen in { $field } sind nicht paarig
cause-unexpected-text = er hat Text, wo ein Feld stehen sollte
    .field = { $field } hat Text, den Ibis nicht lesen kann
cause-unknown-type = sein Typ ist Ibis nicht bekannt
cause-conflicting = er hat doppelte oder widersprüchliche Feldwerte
    .field = { $field } ist mehrfach angegeben
cause-wrong-cells = er hat mehr oder weniger Zellen als die erste Zeile
cause-missing-field = ein Pflichtfeld ist leer
    .field = { $field } ist leer
cause-no-start = seine Felder stehen vor jeder TY-Zeile
cause-string-definition = eine @string-Definition kann nicht gelesen werden
cause-unreadable = er kann nicht gelesen werden
cause-abbreviation = ein Feld verwendet eine Abkürzung, die nicht definiert ist oder nicht aufgelöst werden kann
    .field = { $field } verwendet eine Abkürzung, die nicht definiert ist oder nicht aufgelöst werden kann
cause-date = ein Datum kann nicht vollständig gelesen werden
    .field = { $field } kann nicht vollständig als Datum gelesen werden
cause-names = eine Namensliste kann nicht gelesen werden
    .field = die Namen in { $field } können nicht gelesen werden
cause-role = eine Rolle hat keine CSL-Entsprechung, daher zählen diese Namen als Mitwirkende
    .field = { $field } hat keine CSL-Entsprechung, daher zählen diese Namen als Mitwirkende
cause-link = ein Link ist privat oder keine Webadresse und wird daher weggelassen
    .field = { $field } ist privat oder keine Webadresse und wird daher weggelassen
cause-inheritance = ein Querverweis fehlt oder verweist im Kreis
    .field = { $field } nennt einen Eintrag, der fehlt oder im Kreis verweist
cause-inheritance-limit = Ibis hat seine Grenze für verknüpfte Einträge erreicht
    .field = { $field } erreicht die Grenze von Ibis für verknüpfte Einträge
cause-too-large = ein Feld ist zu groß zum Lesen
    .field = { $field } ist zu groß zum Lesen
cause-identifier = eine Kennung ist ungültig
    .field = { $field } ist keine gültige Kennung
cause-language = seine Sprache ist Ibis nicht bekannt
    .field = { $field } nennt eine Sprache, die Ibis nicht kennt
cause-tex-command = er verwendet einen TeX-Befehl, den Ibis nicht versteht; er bleibt wie geschrieben
cause-unknown-field = ein Feld gehört nicht zum Format und wird daher weggelassen
    .field = { $field } ist kein Feld des Formats und wird daher weggelassen
cause-value = ein Wert kann nicht wie geschrieben gelesen werden
    .field = { $field } kann nicht wie geschrieben gelesen werden

## The command line’s own sentences about a refusal.

cli-together-refused = { $output } kann diese Referenzen nicht wie gewünscht zusammen ausgeben. Zitieren Sie sie einzeln, um zu sehen, welche scheitert.
cli-loss-left-out = { $output } kann { $what }{ $target } nicht darstellen, daher wurde dies weggelassen.
cli-loss-flattened = { $output } hat { $what }{ $target } als reinen Text ausgegeben.
cli-writing-without-bibliography = keine .bib-Datei
cli-loss-unknown = { $output } hat { $what }{ $target } auf eine Weise geändert, die diese Ibis-Version nicht benennen kann. Prüfen Sie die Zitation.
cli-loss-something = einen Teil der Zitation
cli-doctor-about = Bibliotheken und Einstellungen offline prüfen, ohne etwas zu schreiben oder zu senden
cli-doctor-summary = Ibis { $version } · Vertrag { $contract } · { $methods } Engine-Methoden, { $outputs } Hauptausgaben, { $styles } Zitierstile
cli-spelling-match = (ähnliche Schreibweise)
cli-no-references = Keine geöffnete Bibliothek enthält Referenzen. Fügen Sie Referenzen hinzu, oder öffnen Sie mit --file oder --zotero eine andere Bibliothek. Eine Datei muss im Format { -library-formats } sein. Verwenden Sie für CSV die Vorlage von Ibis.
cli-file-help = Eine Datei im Format { -library-formats } oder einen von Ibis aufgenommenen PDF-Ordner öffnen; für mehrere Bibliotheken wiederholen
cli-folder-kept = { $folder }: { $kept } PDFs in ibis-references.bib aufgenommen ({ $unread } ohne lesbare Angaben); { $retry } später erneut versuchen
cli-folder-not-kept = Ibis hat die PDFs dieses Ordners noch nicht aufgenommen; führen Sie zuerst ibis sources keep --file ORDNER aus.
cli-file-unreadable = Sie kann nicht gelesen werden oder ist keine Bibliothek im Format { -library-formats }; prüfen Sie Berechtigungen und Inhalt.
app-file-dialog-title = Bibliothek wählen
app-file-dialog-formats = { -library-formats }

## The app’s native windows and dialogs, and its words about other apps.

app-window-settings = Ibis-Einstellungen
picker-native-settings-picker = Auswahlfenster
picker-native-window-menu = Fenster
picker-native-close-window = Fenster schließen
app-window-notice = Ibis-Hinweis
app-open-link-title = Link öffnen
app-open-link-copied = Ibis konnte Ihren Browser nicht öffnen. Der Link wurde kopiert: Fügen Sie ihn in Ihren Browser ein.
app-open-link-failed = Ibis konnte Ihren Browser nicht öffnen. Öffnen Sie diesen Link in Ihrem Browser:
app-paste-list-before-27 = Bedienungshilfen
app-paste-list-since-27 = Gerätesteuerung und Datenzugriff
app-agent-steps-chatgpt = In ChatGPT Desktop: Einstellungen › MCP-Server › Server hinzufügen › STDIO, Befehl ibis, Argumente mcp --app-libraries.
app-proposal-add = „{ $title }“ als { $key } hinzufügen.
app-proposal-fields = { $fields } von „{ $label }“ ändern.
app-proposal-tags =
    { $state ->
        [add] Die Tags { $add } zu „{ $label }“ hinzufügen.
        [remove] Die Tags { $remove } von „{ $label }“ entfernen.
       *[both] Die Tags { $add } zu „{ $label }“ hinzufügen und die Tags { $remove } entfernen.
    }

# Why a proposed library change was not recorded, shown or applied. $field
# names a field or tag as the proposal did; $count and $kib are numbers.
-proposal-ask-again = Bitten Sie die App, die den Vorschlag erstellt hat, die Änderung erneut vorzuschlagen.
-proposal-choose-library = Suchen Sie mit `proposals` nach einer offenen .bib-Bibliothek oder bitten Sie die Person, eine zu öffnen.
-proposal-edit-input = Überarbeiten Sie den Vorschlag und versuchen Sie es erneut.
refusal-unknown-proposal = Kein Vorschlag hat diese ID.
    .cli = Führen Sie `ibis proposals list` mit denselben Optionen aus, um die ID zu finden.
    .app = Öffnen Sie „Vorgeschlagene Änderungen“ erneut, um die Liste zu aktualisieren.
refusal-proposal-unreadable = Die Datei dieses Vorschlags kann nicht gelesen werden.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
    .agent = Schlagen Sie die Änderung erneut vor.
refusal-store-unreadable = Ibis konnte den Ordner mit den Vorschlägen nicht lesen. Prüfen Sie die Leseberechtigung von Ibis und versuchen Sie es erneut.
refusal-not-saved = Der Vorschlag konnte nicht neben den Einstellungen von Ibis gespeichert werden. Prüfen Sie, ob Ibis dort schreiben darf, und versuchen Sie es erneut.
refusal-too-many = Es warten bereits { $count } Vorschläge.
    .cli = Mit `ibis proposals list` finden Sie Vorschläge, die Sie ablehnen können.
    .app = Übernehmen Sie einen wartenden Vorschlag oder lehnen Sie ihn ab, um Platz zu schaffen.
    .agent = Listen Sie Ihre Vorschläge mit `proposals` auf und bitten Sie die Person, sie in Ibis zu prüfen.
refusal-too-large = Ein Vorschlag umfasst höchstens { $kib } KiB.
    .agent = Kürzen Sie die vorgeschlagene Änderung oder teilen Sie sie in kleinere Vorschläge auf.
refusal-decided = Dieser Vorschlag wurde bereits übernommen oder abgelehnt.
refusal-invalid = Ein Feld in diesem Vorschlag kann nicht verwendet werden.
    .agent = { -proposal-edit-input }
refusal-invalid-field = `{ $field }` kann in einem Vorschlag nicht verwendet werden.
    .agent = { -proposal-edit-input }
refusal-no-target = Wählen Sie eine Bibliothek aus, zu der der Eintrag hinzugefügt werden soll.
    .agent = { -proposal-choose-library }
refusal-unknown-library = Die angegebene Bibliothek ist nicht geöffnet.
    .agent = { -proposal-choose-library }
refusal-unknown-reference = Die Referenz ist in keiner geöffneten Bibliothek; suchen Sie erneut.
refusal-library-kind = Ibis kann das Hinzufügen von Referenzen nur für .bib-Dateien vorschlagen.
    .app = Wählen Sie eine .bib-Bibliothek für die neue Referenz aus.
    .agent = { -proposal-choose-library }
refusal-library-kind-edit = Ibis kann Änderungen an bestehenden Referenzen nur für .bib-Dateien vorschlagen.
    .app = Bearbeiten Sie die Referenz in der App, mit der Sie diese Bibliothek verwalten.
    .agent = Bitten Sie die Person, die Referenz in der App zu bearbeiten, mit der sie diese Bibliothek verwaltet.
refusal-already-in-library = Eine Referenz mit dieser DOI ist bereits in der Bibliothek.
    .agent = Suchen Sie in der Bibliothek nach der vorhandenen Referenz.
refusal-not-open = Die Bibliothek ist in Ibis nicht geöffnet.
    .app = Öffnen Sie die Bibliothek des Vorschlags in Ibis und versuchen Sie es erneut.
refusal-changed = Die Referenz hat sich seit dem Vorschlag geändert; nichts wurde geschrieben.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
    .agent = Suchen Sie nach der aktuellen Referenz und schlagen Sie die Änderung erneut vor.
refusal-folder-changed = Der PDF-Ordner oder seine Bibliografie hat sich beim Erfassen der PDFs geändert. Führen Sie den Befehl erneut aus.
refusal-key-taken = Ein anderer Eintrag verwendet jetzt diesen Zitierschlüssel; nichts wurde geschrieben.
    .cli = { -proposal-ask-again }
    .app = { -proposal-ask-again }
refusal-no-free-key = Alle infrage kommenden Zitierschlüssel sind in dieser Bibliothek belegt.
    .agent = { -proposal-choose-library }
refusal-damaged = Ein Eintrag in der Datei kann nicht gelesen werden, daher lässt Ibis die Datei unverändert.
    .cli = Prüfen Sie die Bibliothek auf fehlerhafte Einträge.
    .app = Prüfen Sie die Bibliothek auf fehlerhafte Einträge.
    .agent = Bitten Sie die Person, die Bibliothek auf fehlerhafte Einträge zu prüfen.
refusal-repeated = Zwei Einträge in der Datei haben diesen Zitierschlüssel.
    .cli = Geben Sie jedem Eintrag in der Bibliotheksdatei einen eindeutigen Schlüssel.
    .app = Geben Sie jedem Eintrag in der Bibliotheksdatei einen eindeutigen Schlüssel.
    .agent = Bitten Sie die Person, jedem Eintrag in der Bibliotheksdatei einen eindeutigen Schlüssel zu geben.
refusal-not-literal = Ein Feld verwendet ein @string-Makro oder verbindet Werte, daher bearbeitet Ibis es nicht.
    .cli = Prüfen Sie das Feld in der Bibliotheksdatei.
    .app = Prüfen Sie das Feld in der Bibliotheksdatei.
    .agent = Bitten Sie die Person, das Feld in der Bibliotheksdatei zu prüfen.
refusal-not-literal-field = `{ $field }` verwendet ein @string-Makro oder verbindet Werte, daher bearbeitet Ibis es nicht.
    .cli = Prüfen Sie das Feld in der Bibliotheksdatei.
    .app = Prüfen Sie das Feld in der Bibliotheksdatei.
    .agent = Bitten Sie die Person, das Feld in der Bibliotheksdatei zu prüfen.
refusal-unreadable = Die Bibliotheksdatei kann nicht als Text gelesen werden.
    .cli = Prüfen Sie, ob die Bibliotheksdatei noch am selben Ort und lesbar ist.
    .app = Prüfen Sie, ob die Bibliotheksdatei noch am selben Ort und lesbar ist.
    .agent = Bitten Sie die Person, zu prüfen, ob die Bibliotheksdatei noch am selben Ort und lesbar ist.
refusal-backup-failed = Eine Sicherung der Datei konnte nicht geschrieben werden; nichts wurde geändert.
    .app = Prüfen Sie, ob Ibis in seinen Einstellungsordner schreiben darf und noch Speicherplatz frei ist. Versuchen Sie es dann erneut.
refusal-not-written = Die Datei konnte nicht geschrieben werden; nichts wurde geändert.
    .app = Prüfen Sie, ob die Bibliotheksdatei beschreibbar und noch Speicherplatz frei ist. Versuchen Sie es dann erneut.

app-update-question-title = Automatisch nach Updates suchen?
app-update-question = Ibis kann einige Male am Tag auf GitHub nach einer neueren Version suchen. Dabei fragt es GitHub nach einer kleinen Datei; GitHub sieht Ihre IP-Adresse und die Ibis-Version. Nichts aus Ihrer Bibliothek wird gesendet, und Updates werden nur installiert, wenn Sie zustimmen.
app-update-check-automatically = Automatisch suchen
app-update-not-now = Nicht jetzt
app-update-offer-title = Ein Update ist verfügbar
app-update-offer = Ibis { $version } ist verfügbar; Sie haben { $current }.
app-update-offer-steps = Ibis lädt es herunter, prüft seine Signatur, installiert es und startet neu.
app-update-offer-question = Dieses Update installieren und Ibis neu starten?
app-update-later = Später
app-update-install = Installieren und neu starten
app-update-cancel = Abbrechen
app-update-install-failed = Das Update konnte nicht installiert werden. Versuchen Sie es später erneut. Falls Ibis nicht mehr startet, laden Sie die neueste Version von GitHub herunter und installieren Sie sie neu.
app-updates-title = Updates

## The app’s own sentence about a citation.

app-loss = Diese Ausgabe kann { $what } nicht vollständig darstellen.
    .named = Diese Ausgabe kann { $what } für { $reference } nicht vollständig darstellen.
app-project-warning =
    { $state ->
        [missing] { $key } fehlt in { $name }.
        [repeated] { $key } kommt in { $name } mehrfach vor.
        [may-name-another] { $key } bezeichnet in { $name } möglicherweise eine andere Referenz.
       *[could-not-check] { $key } konnte nicht mit { $name } abgeglichen werden.
    }
app-tray-open = Ibis öffnen
app-tray-open-shortcut = Ibis öffnen ({ $shortcut })
app-key-control = Control
app-key-option = Wahl
app-key-shift = Umschalt
app-key-command = Befehl
app-tray-choose = Datei wählen…
app-tray-zotero = Zotero
app-tray-retry = Erneut versuchen
app-tray-settings = Einstellungen…
app-tray-update = Nach Updates suchen…
app-tray-help = Hilfe
app-tray-quit = { $name } beenden
app-tray-shortcuts = Tastaturkürzel
app-tray-whats-new = Neuigkeiten
app-tray-problem = Problem melden…
app-tray-idea = Idee vorschlagen…
app-tray-reset = Fensterposition zurücksetzen
app-tray-fix-library = In Ibis korrigieren…
app-tray-library = Bibliothek
app-tray-no-library = Noch keine Bibliothek
app-tray-opening = { $name } wird geöffnet…
app-tray-reading = { $name } wird gelesen…
app-tray-count = { $name } · { $count } Referenzen
app-tray-count-one = { $name } · 1 Referenz
app-tray-zotero-newer = Zotero braucht eine neuere Ibis-Version
app-tray-file-missing = { $name } wurde verschoben oder ist nicht verfügbar
app-tray-file-unreadable = { $name } konnte nicht gelesen werden
app-tray-file-format = { $name } ist keine unterstützte Bibliotheksdatei
app-tray-file-csv = { $name } braucht die CSV-Vorlage von Ibis
app-tray-file-too-large = { $name } ist größer als 64 MiB
app-tray-file-invalid = { $name } enthält keine gültigen Referenzen
app-tray-zotero-sharing = Zotero gibt Ihre Bibliothek nicht frei
app-tray-zotero-profile = Wählen Sie, welches Zotero-Profil gelesen wird
app-tray-zotero-missing = Keine Zotero-Bibliothek gefunden
app-tray-zotero-closed = Öffnen Sie Zotero, um Ihre Bibliothek zu lesen
app-tray-zotero-no-space = Geben Sie Speicherplatz frei oder öffnen Sie Zotero, um Ihre Bibliothek zu lesen
app-tray-zotero-group-missing = Eine gewählte Zotero-Gruppenbibliothek wurde nicht gefunden
app-tray-zotero-work-limit = Das Lesen von Zotero hat zu lange gedauert
app-tray-not-updated = { $name } konnte nicht aktualisiert werden
app-tray-not-opened = { $name } konnte nicht geöffnet werden
app-tray-version-copy-failed = Die Version konnte nicht kopiert werden. Versuchen Sie es erneut.
app-tray-fix-problem = 1 Problem beheben…
app-tray-fix-problems = { $count } Probleme beheben…
app-tray-fix-failed = Die Bibliothek konnte nicht zum Bearbeiten geöffnet werden. Öffnen Sie sie im Ibis-Fenster, um es erneut zu versuchen.
app-tray-spoken-reading = Ibis, liest { $name }
app-tray-spoken-problem = Ibis, { $problem }
app-tray-spoken-empty = Ibis, noch keine Bibliothek
app-tray-spoken-ready = Ibis, bereit
app-saved-title = Seit dem Öffnen zitierte Referenzen speichern
app-saved-file = Seit dem Öffnen zitiert
app-saved-page = Seit dem Öffnen zitierte Referenzen

## The app’s own words when a library cannot be read or a command fails.
# Most are joined into a longer line, so they start in lower case.

app-library-file-unavailable = die Bibliotheksdatei ist nicht verfügbar; laden Sie eine reine Online-Datei zuerst auf diesen Computer
app-library-file-invalid = die Bibliotheksdatei enthält keine gültigen Referenzdaten
app-library-file-too-large = die Bibliotheksdatei ist größer als 64 MiB
app-library-file-unfinished = die Bibliotheksdatei enthält einen unvollständigen Eintrag
app-library-work-limit = die Bibliothek hat die Grenze für den Ladeaufwand überschritten
app-library-file-not-utf8 = die Bibliotheksdatei ist kein UTF-8-Text; speichern Sie sie als UTF-8
app-library-zotero-sharing-off = Zotero ist geöffnet, gibt Ihre Bibliothek aber nicht für andere Apps frei
app-library-zotero-profile-needed = wählen Sie, welches Zotero-Profil gelesen wird
app-library-zotero-closed = die Zotero-Bibliothek kann nicht gelesen werden, solange Zotero geschlossen ist
app-library-zotero-not-found = auf diesem Computer wurde keine Zotero-Bibliothek gefunden
app-library-zotero-no-space = es ist nicht genug Speicherplatz frei, um Zotero bei geschlossener App zu lesen; geben Sie Speicherplatz frei oder öffnen Sie Zotero
app-library-zotero-cancelled = das Lesen von Zotero wurde abgebrochen
app-library-zotero-work-limit = das Lesen von Zotero hat seine Aufwandsgrenze überschritten
app-library-zotero-newer = diese Zotero-Datenbank ist neuer, als Ibis lesen kann
app-library-zotero-group-not-found = eine gewählte Zotero-Gruppenbibliothek wurde nicht gefunden; prüfen Sie, ob Sie noch Mitglied sind
app-library-zotero-try-again = Zotero konnte gerade nicht gelesen werden; versuchen Sie es erneut
app-library-not-local = die gewählte Datei liegt nicht auf diesem Computer
app-library-stopped = das Laden der Bibliothek wurde unerwartet beendet
# $error is one of the sentences above.
app-library-kept = { $error }; die zuletzt geladenen Referenzen bleiben erhalten
# $name is the library’s name; $error is one of the sentences above.
app-library-retained-not-updated = die beibehaltene Bibliothek { $name } konnte nicht aktualisiert werden: { $error }
app-library-not-refreshed = Die Bibliothek konnte nicht neu geladen werden; die zuletzt geladenen Referenzen bleiben erhalten.
app-library-retained-not-refreshed = Die beibehaltene Bibliothek konnte nicht neu geladen werden; die zuletzt geladenen Referenzen bleiben erhalten.
app-library-not-saved = die Bibliothek wurde geöffnet, konnte aber nicht für das nächste Mal gespeichert werden: { $error }
app-command-snapshot-changed = Die Bibliothek hat sich geändert. Suchen Sie erneut.
app-command-library-loading = Bibliothek wird geladen
app-command-cancelled = abgebrochen
app-command-shared-citation-key = der Zitierschlüssel `{ $key }` gehört zu mehr als einer Referenz
app-command-citation-too-large = Diese Zitation enthält zu viel Text. Kürzen Sie ihre Felder oder entfernen Sie Referenzen.
app-command-not-prepared = Die Zitation konnte nicht vorbereitet werden. Versuchen Sie es erneut.
app-command-source-not-opened = Die Quelle konnte nicht geöffnet werden. Versuchen Sie es erneut.
## The app’s interface.

# Citation language and text direction have separate owners.
# Settings interface copy; native guidance and key notation keep their owners.
settings-pages = Einstellungsseiten
settings-general = Allgemein
settings-shortcuts = Tastenkürzel
settings-about = Über Ibis
settings-about-description = Version, Updates und woher Ibis kommt.

general-login-label =
    { $platform ->
        [macos] Beim Anmelden öffnen
        [windows] Beim Anmelden starten
       *[linux] Beim Anmelden starten
    }
general-login-description = Hält das Tastenkürzel nach einem Neustart bereit.
general-login-place =
    { $platform ->
        [macos] Systemeinstellungen > Allgemein > Anmeldeobjekte
        [windows] Einstellungen > Apps > Autostart
       *[linux] den Autostart-Einstellungen Ihrer Arbeitsumgebung
    }
general-login-when =
    { $platform ->
        [windows] anmelden
       *[other] anmelden
    }
general-login-unsupported = Dieses System kann { $name } nicht starten, wenn Sie sich { general-login-when }. Sie können es in { general-login-place } hinzufügen.
general-login-needs-approval = macOS wartet auf Ihre Zustimmung. Aktivieren Sie { $name } in { general-login-place }.
general-login-refused = Das System hat { $name } diese Änderung nicht erlaubt. Sie können sie in { general-login-place } vornehmen.
general-login-unreadable = { $name } konnte nicht prüfen, ob es startet, wenn Sie sich { general-login-when }. Sehen Sie in { general-login-place } nach.

general-menu-bar-label = In der Menüleiste anzeigen
general-language = Sprache
general-language-description = Menüs, Einstellungen und Meldungen, ab dem nächsten Öffnen von Ibis. Zitationen behalten die Sprache ihres Stils.
general-language-system = Wie das System
general-language-words = sprache oberfläche übersetzung englisch deutsch arabisch language
# Each language by its own name, the same in every catalogue.
language-english = English
language-german = Deutsch
language-arabic = العربية
general-placement = Wo Ibis sich öffnet
general-placement-description = Auf dem Bildschirm, auf dem Sie arbeiten, es sei denn, Sie haben Ibis dort verschoben.
general-placement-words = position bildschirm monitor display ecke mitte zentriert öffnen
general-placement-centre = Mitte
general-placement-top-left = Oben links
general-placement-top-right = Oben rechts
general-placement-bottom-left = Unten links
general-placement-bottom-right = Unten rechts
general-menu-bar-description = Wenn ausgeblendet, öffnen Sie Ibis über „Programme“, um hierher zurückzukehren.
general-paste-title = Einfügen
general-behavior-title = Verhalten
general-paste-label = In die vorherige App einfügen
general-paste-key =
    { $platform ->
        [macos] ⌘V
       *[other] Ctrl+V
    }
general-paste-description = Sonst kopiert Ibis, und Sie drücken { general-paste-key }.
# $list is the system's permission-list name, not an interface message code.
general-paste-allowed = Erlaubt unter Datenschutz & Sicherheit › { $list }
general-paste-allowed-note = Ibis drückt ⌘V einmal, nur in der App, aus der Sie es geöffnet haben.
general-paste-guide-title = Ibis das Einfügen erlauben
general-paste-guide-why = Um sicher einzufügen, braucht Ibis Zugriff auf Bedienungshilfen, um zu prüfen, welches Fenster die Tastatur hat, und die Erlaubnis, ⌘V zu drücken. Was Sie tippen, liest Ibis nie.
general-paste-step-open = Klicken Sie auf „Systemeinstellungen öffnen“.
general-paste-step-allow = Aktivieren Sie { $name } unter Datenschutz & Sicherheit › { $list }.
general-paste-step-return = Kehren Sie zu Ihrem Dokument zurück und öffnen Sie Ibis erneut. Drücken Sie Enter, um es noch einmal zu versuchen.
general-paste-pane = Datenschutz & Sicherheit › { $list }
general-paste-open = Systemeinstellungen öffnen
general-paste-drag = { $name } nach { $list } ziehen
general-paste-drag-keyboard = Drücken, um die Systemeinstellungen zu öffnen.
general-paste-drag-fallback = Falls Ibis nicht in der Liste steht, ziehen Sie es hinein.
general-paste-drag-failed = Diese Kopie von Ibis ließ sich nicht ziehen. Fügen Sie sie in den Systemeinstellungen mit der Taste + hinzu.
general-paste-waiting = Warten auf Ihre Zustimmung…
general-paste-unreadable = Ibis konnte diese Berechtigung nicht lesen, daher bleibt das Einfügen aus, bis es möglich ist.
general-paste-granted = Einfügen ist aktiviert.
general-paste-return-failed = Rückkehr zu Ihrer Zitation nicht möglich. Öffnen Sie Ibis mit Ihrem Tastenkürzel, um fortzufahren.
general-paste-stale = Schon aktiviert, aber es wartet noch? Entfernen Sie { $name } in den Systemeinstellungen aus der Liste, fügen Sie diese Kopie erneut hinzu und aktivieren Sie sie.

general-rows = Zeilen
general-rows-description = Kompakte Zeilen zeigen mehr Referenzen, je eine Zeile; komfortable geben jeder Referenz zwei Zeilen.
general-density-compact = Kompakt
general-density-comfortable = Komfortabel

shortcuts-open = Ibis öffnen
shortcuts-description = Klicken Sie auf das Tastenkürzel und geben Sie ein neues ein. Esc bricht ab.
shortcuts-record = Tastenkürzel eingeben…
shortcuts-use-default = Standard verwenden
shortcuts-change = Ändern…
shortcuts-not-assigned = Nicht zugewiesen
shortcuts-configure-failed = Die Arbeitsumgebung konnte ihre Tastenkürzel-Einstellungen nicht öffnen. Versuchen Sie es erneut.
shortcuts-failed = Ibis konnte das Tastenkürzel nicht ändern. Versuchen Sie es erneut.
shortcuts-read-failed = Ibis konnte das Tastenkürzel nicht prüfen. Öffnen Sie die Einstellungen erneut.
shortcuts-refusal-invalid = Ibis kann diese Tastenkombination nicht verwenden. Versuchen Sie ein anderes Tastenkürzel.
shortcuts-refusal-needs-modifier = Nehmen Sie Control, Alt oder Command dazu. Shift allein reicht nicht.
shortcuts-refusal-app-shortcut = Dieses Tastenkürzel ist für App-Befehle reserviert. Nehmen Sie eine weitere Sondertaste dazu.
shortcuts-refusal-alt-gr = Ctrl+Alt mit einer Schreibtaste dient unter Windows als AltGr. Versuchen Sie ein anderes Tastenkürzel.
shortcuts-refusal-in-use = Eine andere App oder das System verwendet dieses Tastenkürzel womöglich. Versuchen Sie ein anderes.
shortcuts-refusal-unavailable = Globale Tastenkürzel sind in dieser Arbeitsumgebung nicht verfügbar. Öffnen Sie Ibis über Ihren Programmstarter.
shortcuts-refusal-not-saved = Das Tastenkürzel funktioniert jetzt, aber Ibis konnte es nicht für später speichern. Versuchen Sie es erneut.
shortcuts-command-format = Format ändern
shortcuts-command-pin = Fenster offen halten
shortcuts-command-keys = Tastaturkürzel
shortcuts-command-more = Mehr
shortcuts-command-change-last = Letzte Zitation ändern
shortcuts-command-source = Quelle öffnen
shortcuts-command-settings = Einstellungen
shortcuts-command-quit = Ibis beenden
shortcuts-picker-conflict = Dieses Tastenkürzel ist für „{ $command }“ vergeben. Versuchen Sie ein anderes.
shortcuts-region-search = Beim Suchen
shortcuts-region-library = Ihre Bibliothek
shortcuts-region-picker = Überall in Ibis
shortcuts-region-menu = In einem Menü
shortcuts-region-system = Ibis öffnen
shortcuts-region-citation = In der Zitation
shortcuts-action-change-last = Gerade eingefügte Zitation bearbeiten
shortcuts-action-source = Quelle öffnen
shortcuts-action-choose-source = Bibliothek wählen
shortcuts-action-reveal-source = Quelle im Ordner zeigen
shortcuts-action-open = Aus jeder App
shortcuts-action-copy = Zitation einfügen oder kopieren
shortcuts-action-paste = Zitation einfügen
shortcuts-action-copy-only = Zitation kopieren
shortcuts-action-add = Weitere Referenz hinzufügen
shortcuts-action-move = Durch die Ergebnisse gehen
shortcuts-action-pick = Ergebnis über seine Nummer einfügen oder kopieren
shortcuts-action-peek = Kurzansicht der markierten Referenz
shortcuts-action-escape = Zurück oder ausblenden
shortcuts-action-format = Format ändern
shortcuts-action-more = Weitere Aktionen
shortcuts-action-pin = Fenster offen halten
shortcuts-action-keys = Tastaturkürzel anzeigen
shortcuts-action-settings = Einstellungen öffnen
shortcuts-action-quit = Ibis beenden
shortcuts-action-undo = Rückgängig
shortcuts-action-text-size = Text vergrößern oder verkleinern
shortcuts-action-citation-move = Zwischen Referenzen wechseln
shortcuts-action-citation-remove = Referenz entfernen
shortcuts-action-citation-reorder = Referenz verschieben
shortcuts-action-citation-open = Details anzeigen
shortcuts-action-citation-focus = Zur Zitation wechseln
shortcuts-action-inspector = Zitationsdetails anzeigen
shortcuts-action-send = An Ihren Editor senden
shortcuts-action-menu-move = Durch das Menü gehen
shortcuts-action-menu-ends = Zum ersten oder letzten Menüpunkt
shortcuts-action-menu-choose = Menüpunkt wählen
shortcuts-action-menu-leave = Menü verlassen und zur Suche zurückkehren
shortcuts-or = oder
# Keys named by a word, as the system's own menus and keyboards write them.
key-enter = Eingabe
key-escape = Esc
key-home = Pos1
key-end = Ende
key-tab = Tab
key-space = Leertaste
key-backspace = Rücktaste
key-delete = Entf
key-plus = Plus
key-minus = Minus
key-ctrl = Strg
key-alt = Alt
key-shift = Umschalt
key-super = Super
key-click = Klick
key-click-held = Klick

about-automatic = Automatisch nach Updates suchen
about-automatic-description = Liest einige Male am Tag eine kleine Update-Datei von GitHub.
about-check = Nach Updates suchen
about-check-label = Nach Updates suchen
about-check-description = Prüfen, ob eine neuere Version von Ibis bereitsteht.
settings-advanced-title = Erweitert
settings-reset-words = zurücksetzen standard wiederherstellen voreinstellung
about-checking = Suche läuft…
about-check-failed = Ibis konnte nicht nach Updates suchen. Versuchen Sie es erneut.
about-check-newest = Sie haben die neueste Version.
about-check-busy = Ibis sucht bereits, oder ein Update wartet auf Ihre Antwort.
about-copied = Kopiert
about-copy-failed = Die Version konnte nicht kopiert werden.
about-version = Version { $version }
about-mission = Ein Tastenkürzel, um in jeder App aus Ihrer Bibliothek zu zitieren.
about-updates = Updates
about-links = Links
about-website = Website
about-source = Quellcode
about-problem = Problem melden
about-licences = Lizenzen
about-free = Freie Software unter der AGPL 3.0
about-unavailable = Diese Version aktualisiert sich nicht selbst.


# Settings window pages and visual choices.
settings-look = Erscheinungsbild
settings-citations = Zitationen
attachment-text-title = Anhangstext
attachment-text-label = KI-Apps Anhangstext lesen lassen
attachment-text-description = Dieser Schalter gilt für verbundene KI-Apps: Sie können Anhangstext mit Seitenzahlen lesen, nie die Dateien. Ein Terminal, in dem ibis läuft, braucht ihn nicht.
attachment-text-words = anhang anhänge volltext text pdf
attachment-text-empty = Keine Bibliothek geöffnet.
attachment-text-loading = Bibliotheksoptionen werden gelesen…
attachment-text-outside-label = Anhangstext außerhalb des Bibliotheksordners erlauben
attachment-text-outside-description = Erlaubt der Befehlszeile und KI-Apps, Anhänge außerhalb des Ordners der Bibliotheksdatei oder des Zotero-Speicherordners auf diesem Computer zu lesen.
settings-search = Suchen
settings-no-results = Keine Einstellungen zu „{ $query }“
settings-results = Suchergebnisse
settings-results-count = { $count ->
    [one] 1 Ergebnis
   *[other] { $count } Ergebnisse
    }
settings-read-failed = Ibis konnte Ihre Einstellungen nicht lesen; öffnen Sie die Einstellungen erneut, um es noch einmal zu versuchen.
settings-forget-failed = Ibis konnte das Format der App nicht vergessen; die gespeicherte Wahl wird angezeigt.
settings-open-failed = Ibis konnte die Einstellungsdatei nicht öffnen.
settings-choice-failed = Ibis konnte diese Wahl nicht speichern; der gespeicherte Wert wird angezeigt.
settings-startup-words = autostart start neustart anmelden anmeldung systemstart
settings-paste-words = einfügen kopieren zwischenablage enter
settings-appearance-words = dunkelmodus dunkel hell thema design system
look-appearance = Erscheinungsbild
look-appearance-description = Dem System folgen oder Hell bzw. Dunkel für Ibis wählen.
look-system = System
look-light = Hell
look-dark = Dunkel
look-accent = Akzentfarbe
look-accent-description = Die Farbe von Auswahl und Tasten: die Akzentfarbe des Mac oder Ibis-Scharlachrot.
look-accent-system =
    { $platform ->
        [macos] Wie der Mac
       *[other] Wie das System
    }
look-accent-ibis = Ibis-Scharlachrot
look-accent-words = farbe akzent scharlachrot rot
look-window-size = Fenstergröße
look-window-size-description = Wie groß sich das Fenster öffnet.
look-window-small = Klein
look-window-medium = Mittel
look-window-large = Groß
look-window-custom = Eigene
look-window-custom-note = Ziehen Sie an einer Kante des Fensters, um die Größe selbst festzulegen. Ibis merkt sie sich für jeden Bildschirm.
look-text-size = Textgröße
look-text-size-description = Die Schriftgröße in Ibis und in den Einstellungen, unabhängig von der Textgröße des Mac.
look-default-text = Standard
look-larger-text = Größer
look-sample-title = Der stille Garten
look-sample-authors = Mira Vale und Noah Reed
look-sample-journal = Zeitschrift für erdachte Studien
citations-default = Standardformat

citations-default-description = Wird in Textverarbeitungen, Mail und Browsern verwendet, wenn sich die App kein Format gemerkt hat.
citations-recent = Letzte Zitationen
citations-recent-description = Zeigt, was Sie zuletzt in jeder App zitiert haben. Beim Ausschalten werden sie vergessen.
citations-recent-words = verlauf zuletzt zitiert hier vergessen datenschutz privatsphäre
citations-editor = Aus Ihrem Editor zitieren
citations-editor-description = Lässt einen Editor auf diesem Computer das Fenster öffnen und die gewählte Zitation empfangen, über Cite as You Write von Better BibTeX. Webseiten können es nicht nutzen, und nichts verlässt diesen Computer.
citations-editor-words = cayw cite as you write vs code vscode better bibtex zotero endpunkt port url editor
citations-editor-address = Adresse für Ihren Editor
citations-editor-address-description = Fügen Sie sie in die Cite-as-You-Write-URL-Einstellung des Editors ein. Wer sie kennt, kann das Fenster auf diesem Computer öffnen; Aus- und wieder Einschalten erzeugt eine neue.
citations-editor-copy = Adresse kopieren
citations-editor-copied = Kopiert
citations-editor-port-in-use = Ein anderes Programm belegt den Port des Endpunkts, daher ist er aus.
citations-editor-bad-setting = Der gespeicherte Endpunkt ist unbrauchbar. Schalten Sie ihn aus und wieder ein, um eine neue Adresse zu erzeugen.
citations-editor-not-saved = Der Endpunkt konnte nicht geändert werden. Versuchen Sie es erneut.

settings-shortcut-restored = Standard-Tastenkürzel wiederhergestellt


settings-newer-title = Von einem neueren Ibis
settings-newer-description = Diese Einstellung ist in dieser Version von Ibis schreibgeschützt.
settings-read-only = Die Einstellungsdatei ist schreibgeschützt; der gespeicherte Wert wird angezeigt.

citations-key-formats = Zitierschlüssel
citations-style-formats = Zitierstile
# Output names and descriptions that are words, not proper names; English repeats the engine's own.
output-name-pandoc-bare = Pandoc ohne Klammern
output-name-keys = Nur Schlüssel
output-name-doi-links = DOI-Links
output-name-scannable-cite = Zotero-RTF/ODF-Scan
output-name-chicago-author-date = Chicago (Autor-Jahr)
output-name-chicago-notes-bibliography = Chicago (Fußnoten)
output-name-gb-numeric = GB/T 7714 (numerisch)
output-name-gb-author-date = GB/T 7714 (Autor-Jahr)
output-name-gb-note = GB/T 7714 (Fußnoten)
output-description-apa = APA (7. Auflage)
output-description-chicago-notes-bibliography = Chicago (Fußnoten und Literaturverzeichnis)
output-description-mla = MLA (9. Auflage)
output-description-gb-numeric = GB/T 7714-2025 (numerisch)
output-description-gb-author-date = GB/T 7714-2025 (Autor-Jahr)
output-description-gb-note = GB/T 7714-2025 (Fußnoten)
citations-filter-formats = Formate filtern
look-sample-title-2 = Pfade durch den Obstgarten
look-sample-title-3 = Eine Karte stiller Orte

# Quiet feedback, counted on this device only.
delight-ibis = Ibis
delight-tip = { $keys } hält Ibis zwischen Zitationen offen.
delight-library100 = 100 Referenzen. Ein guter Anfang.
delight-library1000 = 1.000 Referenzen. Eine solide Bibliothek.
delight-library10000 = 10.000 Referenzen. Eine beachtliche Bibliothek.
delight-empty = Wählen Sie eine Referenz zum Einfügen.
delight-loading = Ihre Bibliothek wird noch geöffnet.
delight-no-library = Öffnen Sie eine Bibliothek, um eine Referenz einzufügen.
delight-zoom = { $percent } %
delight-citation = { $count ->
    [1] Erste Zitation
   *[other] Zitation { NUMBER($count) }
    }
delight-anniversary = Heute ein Jahr mit Ibis.
delight-about-words = ibis version über info thot
canvas-hovered = Zeiger darüber
canvas-pressed = Gedrückt
canvas-focused = Tastaturfokus

general-small-surprises-label = Vogelfakten und Meilensteine
general-small-surprises-description = Ein Fakt, wenn Sie in „Über Ibis“ auf den Vogel klicken, und ein kurzer Hinweis, wenn Sie einen Meilenstein erreichen.
settings-surprises-words = überraschung easter egg spaß kleine überraschungen
delight-name = Benannt nach dem Vogel, den die alten Ägypter mit Thot verbanden, dem Gott der Schrift und Hüter der Aufzeichnungen.
delight-fact-touch = Schneesichler finden ihre Nahrung durch Tasten, wenn sie im Schlamm stochern.
delight-fact-scarlet = Scharlachibisse verdanken ihre Farbe den Pigmenten der Krebstiere, die sie fressen.
delight-fact-mummies = Die alten Ägypter mumifizierten Ibisse als Opfergaben für Thot.
delight-fact-national = Der Scharlachibis ist ein Nationalvogel von Trinidad und Tobago.
delight-fact-flight = Waldrappe wechseln sich an der Spitze ihrer V-Formation ab.
undo-action = Rückgängig: { $action }
undo-remove = { $reference } entfernen
undo-reset-window = Fensterposition zurücksetzen
undo-shortcut = Standard-Tastenkürzel wiederherstellen

undo-removed = { $reference } entfernt

choice-use-zotero = Zotero verwenden
choice-zotero-help = Ihre Zotero-Bibliothek, ob Zotero offen ist oder nicht
choice-library-file = Bibliotheksdatei wählen…
choice-library-file-help = { -library-formats }
choice-drop-file = Oder ziehen Sie eine Datei in dieses Fenster: { -library-formats }.
keys-title = Tastaturkürzel
keys-close = Zurück
keys-action = Aktion
keys-shortcut = Tastenkürzel
shortcuts-filter = Tastenkürzel filtern
shortcuts-empty = Keine passenden Aktionen

choice-use-file = { $name } verwenden
choice-back = Andere Quelle wählen

backup-words = sicherung backup wiederherstellen export import umzug migrieren neuer computer sync icloud onedrive
backup-label = Einstellungen sichern
backup-description =
    { $platform ->
        [linux] Eine Datei zum Wiederherstellen auf einem anderen Computer. Fensterpositionen bleiben auf diesem.
       *[other] Eine Datei zum Wiederherstellen auf einem anderen Computer. Die Apps, in die Ibis einfügen darf, und Fensterpositionen bleiben auf diesem.
    }
backup-button = Sichern…
backup-saved = Gespeichert unter { $file }.
restore-label = Einstellungen wiederherstellen
restore-description = Übernimmt die Einstellungen einer Sicherung. Ibis zeigt zuerst, was sich ändert, und behält die aktuellen Einstellungen als settings.json.bak.
restore-button = Wiederherstellen…
restore-from = Sicherung von { $system }, erstellt mit Ibis { $version }
restore-same = Dieser Computer hat bereits diese Einstellungen.
restore-confirm = Wiederherstellen und neu starten
restore-cancel = Abbrechen
restore-restarting = Ibis wird mit den wiederhergestellten Einstellungen neu gestartet…
backup-change = { $setting }: { $from } → { $to }
backup-note-library = Die Bibliotheksdatei der Sicherung ist nicht auf diesem Computer. Sie bleibt eingetragen, und Ibis findet sie, sobald die Datei da ist.
backup-note-shortcut = Die Sicherung hat kein Tastenkürzel zum Öffnen für dieses System, daher bleibt das dieses Computers.
backup-note-repairs = Die Einstellungsdatei auf diesem Computer ist beschädigt oder fehlt. Die Wiederherstellung schreibt sie neu und bewahrt eine beschädigte Datei daneben auf.
backup-setting-library = Bibliothek
backup-setting-larger-text = Größerer Text
backup-setting-shortcut = Tastenkürzel zum Öffnen unter { $system }
backup-value-default = Standard
backup-value-on = Ein
backup-value-off = Aus
backup-value-zotero = Zotero, Meine Bibliothek
backup-value-zotero-many = { $count } Zotero-Bibliotheken
backup-value-zotero-group = Zotero-Gruppe { $id }
backup-problem-not-downloaded = Die Sicherung ist noch nicht auf diesen Computer geladen. Lassen Sie Ihren Cloud-Ordner sie laden und versuchen Sie es dann erneut.
backup-problem-unreadable = Ibis konnte diese Datei nicht lesen. Nichts wurde geändert.
backup-problem-not-a-backup = Diese Datei ist keine Sicherung der Ibis-Einstellungen. Nichts wurde geändert.
backup-problem-settings-file = Das ist die Einstellungsdatei von Ibis selbst, keine Sicherung. Wählen Sie stattdessen eine Sicherungsdatei.
backup-problem-newer = Diese Sicherung stammt von einer neueren Version von Ibis. Aktualisieren Sie Ibis und versuchen Sie es dann erneut.
backup-problem-invalid = Diese Sicherung ist beschädigt. Nichts wurde geändert.
backup-problem-settings-unusable = Ibis kann seine Einstellungsdatei in diesem Zustand nicht verwenden. Nichts wurde geändert.
backup-problem-busy = Ein anderes Ibis speichert gerade die Einstellungen. Warten Sie, bis es fertig ist, und versuchen Sie es dann erneut. Nichts wurde geändert.
backup-problem-changed = Die Einstellungen haben sich geändert, nachdem die Sicherung angezeigt wurde. Wählen Sie sie erneut.
backup-problem-not-saved = Ibis konnte nicht speichern. Nichts wurde geändert.
## The picker, setup, menus, notices and receipts.

reader-show = PDF anzeigen
reader-hide = PDF ausblenden
reader-cite = Diese Seite zitieren
reader-page-in-view = Sichtbare Seite
reader-thumbnails = Seiten
reader-pages = Seiten des PDF
reader-region = Seiten von { $title }
reader-unknown = PDF-Seite { $index } von { $count } · keine gedruckte Seitenzahl
reader-ask = Welche Zahl steht auf PDF-Seite { $index }?
reader-use-nearby = Für benachbarte Seiten verwenden
peek-label = Details zu { $title }
peek-container = Erschienen in
peek-year = Jahr
peek-type = Typ
peek-key = Zitierschlüssel
locator-act = Akt
locator-appendix = Anhang
locator-article-locator = Artikel
locator-book = Buch
locator-canon = Kanon
locator-chapter = Kapitel
locator-column = Spalte
locator-elocation = Ort
locator-equation = Gleichung
locator-figure = Abbildung
locator-folio = Folio
locator-issue = Heft
locator-line = Zeile
locator-note = Anmerkung
locator-opus = Opus
locator-page = Seite
locator-paragraph = Absatz
locator-part = Teil
locator-rule = Regel
locator-scene = Szene
locator-section = Abschnitt
locator-sub-verbo = Sub verbo
locator-supplement = Supplement
locator-table = Tabelle
locator-timestamp = Zeit
locator-title-locator = Titel
locator-verse = Vers
locator-version = Version
locator-volume = Band
citation-details-locator-type = Art der Fundstelle
locator-short-page = S.
locator-short-chapter = Kap.
locator-short-section = §
locator-short-paragraph = ¶
locator-short-line = Z.
locator-short-verse = V.
locator-short-volume = Bd.
locator-short-book = B.
locator-short-part = T.
locator-short-figure = Abb.
locator-short-table = Tab.
locator-short-note = Anm.
locator-short-appendix = Anh.
locator-short-article-locator = Art.
locator-short-canon = can.
locator-short-column = Sp.
locator-short-elocation = Ort
locator-short-equation = Gl.
locator-short-folio = fol.
locator-short-issue = H.
locator-short-opus = op.
locator-short-rule = R.
locator-short-scene = Sz.
locator-short-sub-verbo = s. v.
locator-short-supplement = Suppl.
locator-short-timestamp = um
locator-short-title-locator = Tit.
locator-short-version = Vers.
locator-short-act = Akt
citation-details-add-prefix = Text davor
citation-details-add-suffix = Text danach
citation-token-remove = { $reference } entfernen
citation-details-locator-filter = Art suchen
citation-details-locator-placeholder = Wert hinzufügen
key-move-left = Nach links verschieben
key-move-right = Nach rechts verschieben
key-edit-citation = Zitationsdetails anzeigen
key-go-to-citation = Zur Zitation wechseln
picker-order-kept = Ihre Reihenfolge bleibt erhalten
picker-your-order = Ihre Reihenfolge
citation-details-prefix = Text davor
citation-details-suffix = Text danach
citation-details-none = Keine
citation-details-page-placeholder = Seite hinzufügen
citation-details-page-hint = Eine Seite, ein Bereich oder mehrere, wie gedruckt
citation-details-form = Anzeigen als
citation-details-examples-unavailable = Keine Beispiele verfügbar
# Why a citation form is left out of the Show as menu.
citation-details-unavailable-entries = Einträge im Literaturverzeichnis haben keine Namensformen. Wählen Sie einen anderen Stil.
citation-details-unavailable-keys-only = „Nur Schlüssel“ hat keine Namensformen. Wählen Sie eine andere Ausgabe.
citation-details-unavailable-textcite = Nicht mit \textcite
citation-details-unavailable-cited-alone = Nur, wenn allein zitiert
citation-details-unavailable-output = Nicht in dieser Ausgabe
citation-details-edit = { $reference } bearbeiten
citation-details-details = Zitationsdetails für { $reference }
operation-failures-recent-history = Letzte Zitationen nicht vergessen. Bitte erneut versuchen.
operation-failures-project = Die .bib-Datei zum Abgleich ließ sich nicht ändern. Erneut versuchen.
operation-failures-row-context = Zeilenmenü nicht geöffnet. Enter kopiert, Shift+Enter fügt hinzu.
operation-failures-status = Ibis konnte Ihre Bibliothek nicht prüfen. Wählen Sie die Datei erneut.
operation-failures-shortcut = Tastenkürzel nicht prüfbar. Ibis über Menüleiste oder Infobereich öffnen.
operation-failures-choose = Die Dateiauswahl wurde nicht geöffnet. Versuchen Sie es erneut.
operation-failures-drop-refused = Nur eine Datei ablegen: { -library-formats }.
operation-failures-zotero = Ibis konnte nicht mit dem Lesen von Zotero beginnen. Versuchen Sie es erneut.
operation-failures-samples = Ibis konnte die Beispielbibliothek nicht ändern. Versuchen Sie es erneut.
operation-failures-zotero-groups = Ibis konnte Ihre Zotero-Gruppen nicht auflisten. Prüfen Sie, ob Zotero geöffnet ist, und versuchen Sie es erneut.
operation-failures-reload = Ibis konnte Ihre Bibliothek nicht neu laden. Wählen Sie die Datei erneut.
operation-failures-hide = Ibis konnte dieses Fenster nicht ausblenden. Versuchen Sie es erneut.
operation-failures-pin = Ibis konnte das Anheften nicht ändern. Versuchen Sie es erneut.
operation-failures-preferences = Fensterauswahl nicht gespeichert. Gilt bis zum Neustart.
operation-failures-window-size = Fenstergröße nicht gespeichert. Sie gilt bis zum Beenden.
operation-failures-quit = Ibis konnte nicht beendet werden. Versuchen Sie es erneut.
operation-failures-update-events = Der Update-Fortschritt ist nicht verfügbar.
operation-failures-picker-events = Suchfeld nicht aktiviert. Starten Sie Ibis neu.
operation-failures-library-events = Ihre Bibliothek wird eventuell nicht automatisch aktualisiert. Wählen Sie „Bibliothek neu laden“, oder starten Sie Ibis neu.
operation-failures-find-sources = Ibis konnte hier keine Bibliotheken suchen. Wählen Sie eine Datei.
operation-failures-open-found = Datei nicht geöffnet. Wählen Sie sie in der neuen Liste erneut.
operation-failures-settings = Die Einstellungen konnten nicht geöffnet werden. Versuchen Sie es erneut.
operation-failures-app-choices = App-Auswahl beschädigt. Wählen Sie „Einfügen“ oder „Nur kopieren“.
operation-failures-app-read = Ibis konnte nicht erkennen, welche App das ist. Ihr Standard gilt.
operation-failures-app-choice = Auswahl für diese App nicht gespeichert. Erneut versuchen.
operation-failures-app-delivery = Format gespeichert, „Einfügen“/„Nur kopieren“ nicht. Erneut wählen.
setup-sources-title = Wo liegen Ihre Referenzen?
setup-sources-line =
    { $platform ->
        [linux] Ibis findet eine Referenz und kopiert ihre Zitation, damit Sie sie dort einfügen, wo Sie schreiben. Ibis liest Ihre Bibliothek nur und ändert sie nie.
       *[other] Ibis findet eine Referenz und fügt ihre Zitation dort ein, wo Sie schreiben. Ibis liest Ihre Bibliothek nur und ändert sie nie.
    }
setup-return-hint = Danach öffnen Sie Ibis in jeder App mit { $shortcut }.
setup-shortcut-unavailable = Das Tastenkürzel ist nicht verfügbar. Öffnen Sie Ibis über die Menüleiste oder den Infobereich.
setup-no-shortcut = Kein Tastenkürzel festgelegt.
setup-open-without-shortcut = Öffnen Sie Ibis über die Menüleiste oder den Infobereich.
setup-opening-line = Ihre Bibliothek wird gelesen. Zotero kann geschlossen bleiben.
setup-samples = Beispielreferenzen ausprobieren
setup-samples-help = Drei erfundene zum Ausprobieren
setup-sample-label = Beispiel
setup-sample-title = Beispielreferenzen
setup-sample-search-help = Suchen Sie nach „lantern“, wählen Sie die Referenz „Lantern gardens“ und drücken Sie Enter.
setup-own-library = Eigene Bibliothek verwenden
setup-own-library-help = Zotero oder eine Bibliotheksdatei statt der Beispiele wählen
setup-empty-line = Fügen Sie Referenzen hinzu oder wählen Sie eine andere Bibliothek.
setup-zotero = Zotero
setup-find-libraries = Auf diesem Computer suchen
setup-found-in = Gefunden in { $folder }
setup-choose-found = Gefundene Bibliothek wählen
setup-found-title = Auf diesem Computer gefundene Bibliotheken
setup-find-help = Zotero und .bib-Dateien in der Nähe
setup-look-again = Erneut suchen
setup-looking = Suche nach Zotero und .bib-Dateien…
setup-cut-short = Einige Ordner waren zu groß, um sie ganz zu durchsuchen. Wählen Sie die Datei selbst, falls sie fehlt.
setup-nothing-found = Keine Zotero- oder .bib-Dateien in der Nähe gefunden. Wählen Sie die Datei selbst.
setup-missing-title = { $name } nicht gefunden
setup-missing-line = Eventuell verschoben, oder das Laufwerk ist nicht verbunden.
setup-find = { $name } suchen
setup-find-file-help = Wählen Sie den aktuellen Ort
setup-unreadable-title = { $name } kann nicht gelesen werden
setup-zotero-closed-title = Öffnen Sie Zotero, um Ihre Bibliothek zu lesen
setup-zotero-closed-line = Normalerweise liest Ibis sie bei geschlossenem Zotero, diesmal aber nicht.
setup-zotero-missing-title = Ibis findet die Zotero-Bibliothek nicht
setup-zotero-missing-line = Falls Zotero nicht installiert ist, installieren Sie es und öffnen Sie es einmal. Ibis macht dann von selbst weiter.
setup-zotero-access-off-title = Zotero gibt Ihre Bibliothek nicht frei
setup-zotero-access-off-waiting = Ibis macht von selbst weiter, sobald die Freigabe aktiv ist.
setup-open-zotero-help = Ibis macht von selbst weiter, sobald Zotero geöffnet ist
setup-open-file-help = Datei in ihrer eigenen App ansehen
setup-add-references-help = Dort Referenzen hinzufügen
setup-zotero-profiles-title = Welches Zotero-Profil?
setup-zotero-profiles-line = Zotero ist geschlossen. Wählen Sie das Profil, das gelesen werden soll.
setup-zotero-profiles-failed = Ibis konnte Ihre Zotero-Profile nicht auflisten. Öffnen Sie Zotero, dann macht Ibis von selbst weiter.
setup-other-title = { $name } kann nicht geöffnet werden
setup-the-library = Bibliothek
setup-try-again = Erneut versuchen
setup-try-again-help = Bibliothek erneut lesen
setup-another = Andere Bibliothek verwenden
setup-another-help = Zotero oder eine andere Bibliotheksdatei
session-menu = Seit dem Öffnen zitiert…
session-title = Seit dem Öffnen zitiert
session-about = Was Sie seit dem Öffnen von Ibis oder seit dem Zurücksetzen dieser Liste mit Ibis kopiert haben, in allen Apps. Ibis sieht Ihr Dokument nicht, gleichen Sie die Liste also damit ab.
session-empty = Noch nichts. Referenzen, die Sie kopieren, erscheinen hier.
session-close = Zurück
session-remove = Entfernen
session-reset = Liste zurücksetzen
session-undo-reset = Zurücksetzen rückgängig machen
session-full = Die Liste ist voll, daher wurden spätere Referenzen nicht hinzugefügt.
session-copy = Literaturverzeichnis kopieren
session-save = Speichern…
session-copied = Literaturverzeichnis kopiert.
session-changed = Die Liste hat sich geändert. Prüfen Sie sie und versuchen Sie es erneut.
session-not-read = Die Referenzen konnten nicht gelesen werden. Versuchen Sie es erneut.
session-not-formatted = Aus diesen Referenzen ließ sich kein Literaturverzeichnis erstellen. Versuchen Sie einen anderen Stil.
session-not-copied = Das Literaturverzeichnis konnte nicht kopiert werden. Versuchen Sie es erneut.
session-not-saved = Das Literaturverzeichnis konnte nicht gespeichert werden. Versuchen Sie es erneut.
writing-title = Projekte
writing-loading = Projekte werden geladen…
writing-saving = Projekt wird gespeichert…
writing-menu = Neues Projekt…
writing-about = Ein Projekt hält sein Zitationsformat und ein optionales Literaturverzeichnis zusammen. Ihre Wahl wird für diese App gespeichert; Änderungen gelten überall, wo Sie dieses Projekt nutzen. Ibis erkennt das Dokument nicht.
writing-session-only = Ibis kann die App nicht erkennen, daher gilt dieses Projekt nur, bis Ibis beendet wird.
writing-close = Schließen
writing-none = Kein Projekt
writing-empty = Noch keine Projekte. Bis Sie eines wählen, gilt das Format Ihrer App.
writing-use = { $name } verwenden
writing-selected = Ausgewähltes Projekt
writing-selected-label = Ausgewählt
writing-format = Format: { $name }
writing-bibliography = Literaturverzeichnis: { $name }
writing-no-bibliography = Kein Literaturverzeichnis ausgewählt.
writing-choose-bibliography = Literaturverzeichnis wählen…
writing-remove-bibliography = Literaturverzeichnis entfernen
writing-create = Neues Projekt…
writing-confirm-create = Projekt erstellen
writing-starts-with = Beginnt mit { $format }. Ändern Sie es nach dem Erstellen des Projekts im Formatmenü.
writing-name = Name des Projekts
writing-rename = Projekt umbenennen…
writing-save-name = Namen speichern
writing-remove = Projekt entfernen…
writing-confirm-remove = Projekt entfernen
writing-cancel = Abbrechen
writing-unsupported = Diese App-Version kann das Format dieses Projekts nicht verwenden.
writing-not-read = Die Projekte konnten nicht gelesen werden. Schließen Sie dieses Fenster und versuchen Sie es erneut.
writing-not-saved = Das Projekt konnte nicht gespeichert werden. Ihre vorherige Wahl gilt weiter. Versuchen Sie es erneut.
writing-bibliography-not-saved = Die Wahl des Literaturverzeichnisses konnte nicht gespeichert werden. Versuchen Sie es erneut.
writing-pick = Ihre Auswahl ändert das Format dieses Projekts.
project-check-retry = Erneut versuchen
project-check-locate = Literaturverzeichnis suchen…
picker-choose-project = Mit einer .bib-Datei abgleichen…
picker-choose-other-project = Mit anderem Literaturverzeichnis abgleichen…
picker-open-zotero = Zotero öffnen
picker-show-in-zotero = In Zotero zeigen
picker-source-changed = Die Bibliothek hat sich geändert; wählen Sie die Referenz erneut.
picker-search-label = Referenzen suchen
picker-search-placeholder = Referenzen suchen…
picker-drag-search-bar = Am Rand der Suchleiste ziehen, um das Fenster zu bewegen
picker-search-error = Suche fehlgeschlagen. „Neu laden“ wählen, dann erneut versuchen.
picker-copy-error = Nichts kopiert. Erneut versuchen oder „Neu laden“ wählen.
picker-snapshot-changed = Die Bibliothek hat sich geändert. Suchen Sie erneut und wählen Sie die Referenzen vor dem Kopieren neu aus.
picker-shared-key = Mehrere Referenzen haben den Schlüssel { $key }, daher kann nicht per Schlüssel zitiert werden. Geben Sie einer davon in Zotero oder Ihrer .bib-Datei einen anderen Schlüssel.
picker-using-shorthand = Verwendet: { $value }
picker-apply = Anwenden
# $word: a word that finds the person’s own cited reference, never a sample one.
picker-shorthand-hint = „{ $word } 417“ für eine Seite, „see { $word }“ für „see“.
picker-kept-words = Als Suchwörter behalten. Für eine Seite setzen Sie ein Komma: „room, 22“.
picker-practice-offer = Probieren Sie das Einfügen zuerst hier aus: Drücken Sie Enter.
# $citation: exactly what Enter would paste.
picker-practice-pasted = { $citation } hier eingefügt. Als Nächstes fügt Enter in Ihre App ein.
picker-search-as-typed = Wie eingegeben suchen
picker-apply-locator = Fundstelle auf { $key } anwenden
picker-copied = Kopiert
picker-key-formats = Schlüsselformate
picker-formatted-styles = Zitierstile
picker-share-formats = Referenzen teilen
picker-filter-formats = Formate filtern
picker-format-for-app = { $format } · { $app }
picker-format-this-paste = { $format } · Nur dieses Mal
picker-format-remembered = Für { $app } gespeichert.
picker-format-suggested = Für { $app } empfohlen.
picker-format-app-default = Für { $app } ist kein Format gespeichert, daher gilt Ihr Standard.
picker-format-no-app = Ibis erkennt nicht, für welche App das ist, daher gilt Ihr Standard.
picker-format-held = { $format } gilt nur für dieses Mal.
picker-pick-replaces = Ihre Auswahl ersetzt es.
picker-pick-remembered = Ihre Auswahl wird für diese App gespeichert.
picker-pick-default = Ihre Auswahl wird Ihr Standard.
picker-pick-this-paste = Ihre Auswahl gilt nur für dieses Mal.
picker-pick-to-remember = Wählen Sie ein Format, um es für diese App zu speichern.
picker-pick-to-set-default = Wählen Sie ein Format, um es als Standard festzulegen.
picker-for-app = Für { $app }
picker-pick-heading-default = Standardformat
picker-pick-heading-this-paste = Nur dieses Mal
picker-paste-from-now-on = Ab jetzt in { $app } einfügen
picker-copy-only = Nur kopieren
picker-preview = Vorschau
picker-citation-details = Zitationsdetails
picker-details = Details
picker-citation-warnings = Zitationswarnungen
picker-window-reset = Fensterposition zurückgesetzt.
picker-undo = Rückgängig
picker-notices = Hinweise
picker-undo-failed = Rückgängig fehlgeschlagen. Bitte erneut versuchen.
picker-preview-pending = Wird formatiert…
picker-preview-error = Diese Zitation konnte nicht formatiert werden. Wählen Sie einen anderen Stil oder kopieren Sie die Zitierschlüssel.
picker-preview-refused = Diese Zitation konnte nicht vorbereitet werden. Schließen und öffnen Sie Ibis erneut, um es noch einmal zu versuchen.
picker-preview-style-error = Formatieren in { $style } fehlgeschlagen. Wählen Sie einen anderen Stil oder kopieren Sie die Schlüssel.
picker-entry-only = Nummern in { $style } hängen vom ganzen Dokument ab, daher kopiert Ibis stattdessen den Eintrag der Referenz.
picker-entry-only-short = Eintrag der Referenz
picker-first-note = Spätere Fußnoten zum selben Werk werden gekürzt, und welche zuerst kommt, hängt vom ganzen Dokument ab. Daher kopiert Ibis die vollständige erste Fußnote.
picker-first-note-short = Vollständige erste Fußnote
picker-shelf-stale = Die Bibliothek hat sich geändert. Sie wird neu gelesen…
picker-dismiss = Schließen
picker-dismiss-warnings = Kopierwarnungen schließen
picker-copy-warnings = Kopierwarnungen
picker-citation-selection = Zitationsauswahl
picker-selected-references = Ausgewählte Referenzen
picker-references = Referenzen
picker-search-results = Suchergebnisse
picker-results = Ergebnisse
picker-no-date = Ohne Datum
picker-added-to-citation = Zur Zitation hinzugefügt
picker-add-named = { $name } zur Zitation hinzufügen
picker-remove-named = { $name } aus der Zitation entfernen
picker-best-matches = Beste Treffer
picker-pasted-list = Eingefügte Liste
picker-paste-into = In { $app } einfügen · { $output }
picker-bar-paste = In { $app } einfügen
picker-bar-paste-entries =
    { $count ->
        [one] Eintrag in { $app } einfügen
       *[other] { $count } Einträge in { $app } einfügen
    }
picker-bar-copy =
    { $count ->
        [one] Kopieren
       *[other] { $count } Zitationen kopieren
    }
picker-bar-copy-entries =
    { $count ->
        [one] Eintrag kopieren
       *[other] { $count } Einträge kopieren
    }
picker-bar-send = An Editor senden
picker-bar-change = Zitation ändern
picker-bar-practice = Hier zum Ausprobieren einfügen
picker-bar-refused = Nicht eingefügt oder kopiert. { $reason }
picker-last-outcome = Letztes Ergebnis: { $outcome }
picker-closest-matches = Nächste Treffer
picker-closest-match = Ungefährer Treffer
picker-spelling-match = Ähnliche Schreibweise
picker-closest-match-warning = „{ $title }“ ist ein ungefährer Treffer: Nur einige Suchwörter passen.
picker-no-matches = Keine passenden Referenzen
picker-no-matches-hint = Versuchen Sie einen Nachnamen, ein Jahr oder weniger Wörter.
picker-filter-obstacle = Keine Treffer mit diesem Filter
picker-filter-obstacle-hint = Ein Tag, Jahr oder Feld in Ihrer Suche schließt alles aus.
picker-remove-filter = Filter entfernen
picker-scope-obstacle-hint = Einige Ihrer Zotero-Bibliotheken sind von dieser Suche ausgeschlossen.
picker-not-in-libraries = Nicht in Ihren Bibliotheken
picker-not-in-libraries-hint = Fügen Sie sie Ihrer Bibliothek hinzu und suchen Sie erneut.
picker-clear-search = Suche löschen
picker-search-all-libraries = Alle Zotero-Bibliotheken durchsuchen
picker-type-to-find = Tippen, um eine Referenz zu finden
picker-type-to-find-hint = Suchen Sie nach Titel, Autor, Jahr, Zitierschlüssel oder DOI.
picker-more = Mehr
picker-keep-my-order = Meine Reihenfolge beibehalten
picker-change-last-citation = Letzte Zitation ändern
picker-paste-reference-list = Literaturliste einfügen
picker-copy-reference-list = Literaturliste kopieren
picker-reference-list-title = Literaturliste
# $app: the app's own name. The list holds this app's citations from this run of Ibis.
picker-reference-list-help =
    { $count ->
        [one] Die Referenz, die Sie seit dem Öffnen von Ibis in { $app } zitiert haben
       *[other] Die { $count } Referenzen, die Sie seit dem Öffnen von Ibis in { $app } zitiert haben
    }
picker-forget-recent = Letzte Zitationen vergessen
picker-change-citation = Zitation ändern · { $format }
picker-keep-open = Fenster offen halten
picker-keep-open-menu = Fenster offen halten
picker-change-format = Format ändern…
picker-drag-citation = Ziehen Sie die Zitation in eine beliebige App
picker-more-title = Weitere Aktionen ({ $keys })
picker-use-zotero = Zotero verwenden
picker-library-region = Bibliothek
picker-picker-actions = Fensteraktionen
picker-library-status = Bibliotheksstatus
picker-reload-library = Bibliothek neu laden
picker-choose-zotero-profile = Zotero-Profil wählen…
picker-zotero-profile = Zotero-Profil
picker-choose-library-menu = Bibliothek wählen…
picker-format-label = Zitationsformat
picker-quit-label = Ibis beenden
picker-settings = Einstellungen…
picker-zotero-libraries = Zotero-Bibliotheken
picker-search-in = Suchen in
picker-all-libraries = Alle Bibliotheken
picker-my-library = Meine Bibliothek
picker-group-library = Gruppe { $id }
picker-keep-one-library = Lassen Sie mindestens eine Zotero-Bibliothek aktiviert.
key-title = Tastaturkürzel
key-close = Schließen
key-footer = Tasten
key-copy = Kopieren
key-copy-as = Als { $format } kopieren
key-copy-several-as = { $count } als { $format } kopieren
key-add-to-citation = Zur Zitation hinzufügen
key-remove-from-citation = Aus Zitation entfernen
key-close-menu = Menü schließen
key-clear-search = Suche löschen
key-hide = Ibis ausblenden
library-live-updates-off = Live-Updates sind nicht verfügbar. Laden Sie die Bibliothek nach Änderungen neu.
row-et-al = et al.
copied-notice-title = Kopiert
copied-notice-plugin-note = Wenn dieses Dokument das Plug-in einer Literaturverwaltung nutzt (Zotero, Mendeley, EndNote, Citavi, Paperpile und andere), fügen Sie über das Plug-in ein.
copied-notice-source = Ibis
copied-notice-notice-failed = Der Hinweis „Kopiert“ konnte nicht angezeigt werden.
# Names inside these sentences arrive isolated (⁨…⁩), so a right-to-left name keeps its place.
reader-indicator = S. { $printed } · PDF-Seite { $index } von { $count }
reader-pdf-page = PDF { $index }
setup-found-on =
    { $platform ->
        [macos] Auf diesem Mac gefunden
        [windows] Auf diesem PC gefunden
       *[other] Auf diesem Computer gefunden
    }
setup-zotero-access-off = Damit Ibis Ihre Referenzen lesen kann, aktivieren Sie in Zotero › Einstellungen › Erweitert „Anderen Anwendungen auf diesem Computer erlauben, mit Zotero zu kommunizieren“.
setup-zotero-open = Zotero ist geöffnet.
setup-zotero-installed = Zotero ist auf diesem Computer installiert.
setup-bib-found =
    { $count ->
        [one] 1 .bib-Datei gefunden.
       *[other] { $count } .bib-Dateien gefunden.
    }
session-remove-named = { $name } entfernen
session-undo-remove = Entfernen rückgängig machen
session-undo-remove-named = Entfernen von { $name } rückgängig machen
session-absent =
    { $count ->
        [one] 1 Referenz ist nicht in der geöffneten Bibliothek und wird ausgelassen.
       *[other] { $count } Referenzen sind nicht in der geöffneten Bibliothek und werden ausgelassen.
    }
session-left-out = Ausgelassen, weil dieser Stil sie nicht ausgeben kann: { $names }.
session-bibliography-in = Literaturverzeichnis in { $style }
session-saved = { $name } gespeichert.
writing-remove-question = { $name } aus Ibis entfernen? Die Datei des Literaturverzeichnisses bleibt, wo sie ist.
writing-remembered = Für { $name } gespeichert.
picker-project-state =
    { $state ->
        [in] In { $name }
        [missing] Fehlt in { $name }
        [could-not-check] { $name } nicht prüfbar
        [repeated] Schlüssel kommt mehrfach vor
       *[may-name-another] Schlüssel könnte eine andere Referenz bezeichnen
    }
picker-project-lines =
    { $count ->
        [one] Zeile { $lines }
       *[other] Zeilen { $lines }
    }
picker-project-reading = { $name } wird gelesen
picker-project-damaged = { $name } konnte nicht vollständig gelesen werden, daher gilt nichts als fehlend.
picker-project-failed = { $name } konnte nicht gelesen werden.
# Why a project bibliography could not be read, after picker-project-failed.
picker-project-problem-unavailable = Die Datei ist nicht vorhanden oder noch nicht auf diesem Computer.
picker-project-problem-not-a-file = Es ist keine Datei.
picker-project-problem-too-large = Die Datei ist größer als 64 MiB.
picker-project-problem-not-utf8 = Die Datei ist kein UTF-8-Text.
picker-project-problem-changing = Die Datei ändert sich noch.
picker-project-problem-not-bibtex = Es muss eine BibTeX- oder BibLaTeX-Datei sein.
picker-project-checking = Abgleich mit { $name }
picker-project-check-lines =
    { $count ->
        [one] Prüfen Sie Zeile { $lines }.
       *[other] Prüfen Sie die Zeilen { $lines }.
    }
picker-stop-project = Abgleich mit { $name } beenden
picker-copy-project-entry = Eintrag für { $name } kopieren
picker-project-entry =
    { $outcome ->
        [copied] Der Eintrag für { $key } wurde so kopiert, wie er in Ihrer Datei steht, samt etwaiger file- oder note-Felder. Prüfen Sie ihn und fügen Sie ihn dann zu { $name } hinzu.
        [no-original] Für diese Referenz gibt es hier keinen ursprünglichen BibTeX-Eintrag. Exportieren Sie sie aus Ihrer Literaturverwaltung im Dialekt Ihres Projekts und behalten Sie den Schlüssel bei.
        [changed] Die Bibliothek oder das Literaturverzeichnis hat sich geändert. Suchen Sie erneut und versuchen Sie es dann noch einmal.
        [repeated] Die Bibliotheksdatei enthält diesen Schlüssel mehrfach, daher weiß Ibis nicht, welcher Eintrag kopiert werden soll.
       *[dependency] Dieser Eintrag hängt von einem crossref, xdata oder @string ab, der nicht mitkopiert werden kann. Exportieren Sie ihn aus Ihrer Literaturverwaltung und behalten Sie den Schlüssel bei.
    }
picker-editor-waiting = Zitieren für Ihren Editor in { $format }. Esc bricht ab.
picker-editor-asking = Zitieren für Ihren Editor. Esc bricht ab.
picker-send = Senden · { $format }
picker-delivery-editor = Editor
picker-delivery-clipboard = Zwischenablage
picker-delivery-paste = Automatisch einfügen
picker-find-source = { $name } suchen…
picker-find-source-unnamed = Suchen…
picker-show-source-folder =
    { $platform ->
        [macos] Im Finder zeigen
       *[other] Im Ordner anzeigen
    }
picker-open-source = { $name } öffnen
picker-open-library = Bibliothek öffnen
picker-source-open-failed =
    { $source ->
        [zotero] Zotero konnte nicht geöffnet werden. Versuchen Sie es erneut.
       *[file] Die Quelldatei konnte nicht geöffnet werden. Versuchen Sie es erneut.
    }
picker-warning-count =
    { $count ->
        [one] 1 Warnung
       *[other] { $count } Warnungen
    }
picker-copy-citations =
    { $count ->
        [one] 1 Zitation kopieren
       *[other] { $count } Zitationen kopieren
    }
picker-entry-output =
    { $count ->
        [one] { $style }-Eintrag
       *[other] { $count } { $style }-Einträge
    }
picker-missing-citations = Nicht in Ihrer Bibliothek: { $keys }
picker-matched-in =
    { $field ->
        [doi] Treffer in der DOI
        [keyword] Treffer in den Schlagwörtern
        [container] Treffer in Zeitschrift oder Buch
        [abstract] Treffer im Abstract
        [identifier] Treffer in ISBN, PubMed-ID oder arXiv-ID
       *[other] Treffer in { $field }
    }
picker-recent-heading =
    { $state ->
        [cited] Hier zuletzt zitiert
       *[added] Zuletzt hinzugefügt
    }
picker-more-references = +{ $count } weitere
picker-result-count =
    { $count ->
        [one] 1 Ergebnis
       *[other] { $count } Ergebnisse
    }
picker-selection-count =
    { $count ->
        [one] 1 Referenz zum Zitieren ausgewählt
       *[other] { $count } Referenzen zum Zitieren ausgewählt
    }
picker-shortcut-not-used = Ihre Tastenkürzel-Einstellung wurde nicht verwendet: { $reason }
picker-shortcut-unavailable = Das Tastenkürzel { $shortcut } ist nicht verfügbar: { $reason }
key-copy-entries =
    { $count ->
        [one] { $style }-Eintrag kopieren
       *[other] { $count } { $style }-Einträge kopieren
    }
library-opening = { $name } wird geöffnet…
library-opening-unnamed = Bibliothek wird geöffnet…
library-reloading = { $name } wird neu geladen…
library-reloading-unnamed = Bibliothek wird neu geladen…
library-zotero-not-read = { $name } konnte nicht gelesen werden. { $reason }
library-open-zotero-then-reload = Öffnen Sie Zotero und laden Sie dann neu.
library-file-not-opened = Die Bibliothek konnte nicht geöffnet werden: { $name }. { $reason } Wählen Sie sie erneut aus oder verbinden Sie das Laufwerk wieder.
library-the-chosen-file = die gewählte Datei
library-empty-title = Keine Referenzen in { $name }
library-empty-title-unnamed = Keine Referenzen in dieser Bibliothek
library-checking = Bibliothek wird geprüft…
library-details-unavailable = Bibliotheksdetails nicht verfügbar
library-none-chosen = Keine Bibliothek gewählt
library-not-opened = nicht geöffnet
library-moved = verschoben
library-not-updated = nicht aktualisiert
library-no-references = keine Referenzen
library-references =
    { $count ->
        [one] { $count } Referenz
       *[other] { $count } Referenzen
    }
library-skipped =
    { $count ->
        [one] { $count } Eintrag übersprungen
       *[other] { $count } Einträge übersprungen
    }
library-to-check = { $count } zu prüfen
library-age-opening = wird geöffnet…
library-age-reloading = wird neu geladen…
library-updated = aktualisiert { $ago }
library-checked-at = geprüft { $ago }, { $time }
library-last-read-at = zuletzt gelesen { $ago }, { $time }
library-zotero-unreachable = Zotero war nicht erreichbar
library-file-unreadable = die Datei konnte nicht gelesen werden
library-where = ({ $lines })
library-key-users = { $count } verwenden den Schlüssel { $key }
library-others-share =
    { $count ->
        [one] 1 weiterer teilt einen Schlüssel
       *[other] { $count } weitere teilen einen Schlüssel
    }
library-could-not-read =
    { $skipped ->
        [one] er war nicht lesbar
       *[other] { $count } waren nicht lesbar
    }
library-skipped-because =
    { $skipped ->
        [one] { $skipped } Eintrag übersprungen: { $reasons }.
       *[other] { $skipped } Einträge übersprungen: { $reasons }.
    }
library-check = Prüfen Sie { $lines }.
library-check-library = Prüfen Sie die Bibliothek.
library-line = Zeile { $line }
library-line-why = Zeile { $line }: { $why }
library-and-more = und { $count } weitere
library-lines = Zeilen { $lines } und { $last }
library-lines-and-others = Zeilen { $lines } und { $last } sowie weitere
library-work-limit = Die Bibliothek ist zu groß, um sie vollständig zu lesen.
library-cloud-synced = Sie liegt in einem Cloud-synchronisierten Ordner, daher können Änderungen verspätet ankommen.
library-zotero-newer = Zotero ist neuer als die Version, mit der diese Ibis-Version getestet wurde. Die Ergebnisse sehen richtig aus, aktualisieren Sie Ibis aber, sobald Sie können.
library-keys-unavailable = Better-BibTeX-Schlüssel konnten nicht gelesen werden, daher fehlen sie bei einigen Einträgen.
time-just-now = gerade eben
time-on = am { $date }
time-short =
    { $unit ->
        [now] jetzt
        [minute] { $n } Min.
        [hour] { $n } Std.
       *[day] { $n } Tg.
    }
# Short word for a CSL item type, read by screen readers; other types say none.
row-item-type =
    { $type ->
        [article] Artikel
        [article-journal] Artikel
        [article-magazine] Artikel
        [article-newspaper] Artikel
        [book] Buch
        [chapter] Kapitel
        [dataset] Datensatz
        [entry-dictionary] Eintrag
        [entry-encyclopedia] Eintrag
        [manuscript] Manuskript
        [paper-conference] Konferenzbeitrag
        [patent] Patent
        [post] Beitrag
        [post-weblog] Blogbeitrag
        [report] Bericht
        [review] Rezension
        [review-book] Rezension
        [software] Software
        [speech] Vortrag
        [standard] Norm
        [thesis] Abschlussarbeit
        [webpage] Webseite
       *[other] {""}
    }
copied-notice-pasted-into = In { $app } eingefügt
copied-notice-pasted = Eingefügt
# Every "copied, not pasted" sentence says why and ends with its fix, written
# with ⌘V; other systems show Ctrl+V.
copied-notice-reason =
    { $reason ->
        [paste-off] Kopiert. Einfügen ist in den Einstellungen deaktiviert. Drücken Sie ⌘V zum Einfügen.
        [unsupported-system] Kopiert. Ibis kann auf diesem System noch nicht einfügen. Drücken Sie ⌘V zum Einfügen.
        [no-target] Kopiert. Es gab keine App zum Einfügen. Drücken Sie ⌘V zum Einfügen.
        [unknown-app] Kopiert. Automatisches Einfügen ist für diese App nicht freigegeben. Drücken Sie ⌘V zum Einfügen.
        [copy-only] Kopiert. Für diese App ist „Nur kopieren“ eingestellt. Drücken Sie ⌘V zum Einfügen.
        [nonstandard-shortcut] Kopiert. Die Einfügetaste dieser App macht etwas anderes. Verwenden Sie ihren eigenen Einfügebefehl.
        [target-changed] Kopiert. Die Ziel-App oder das Fenster hat sich geändert. Drücken Sie ⌘V zum Einfügen.
        [clipboard-changed] Kopiert. Die Zwischenablage hat sich inzwischen geändert. Kopieren Sie die Zitation erneut.
        [paste-expired] Kopiert. Das Einfügen ist abgelaufen. Drücken Sie ⌘V zum Einfügen.
        [no-permission] Kopiert. Ibis braucht Ihre Erlaubnis zum Einfügen. Drücken Sie ⌘V zum Einfügen.
        [accessibility-required] Kopiert. Ibis braucht Zugriff auf die Bedienungshilfen, um sicher einzufügen. Drücken Sie ⌘V zum Einfügen.
        [terminal] Kopiert. Nicht in ein Terminal eingefügt. Drücken Sie ⌘V zum Einfügen.
        [remote-desktop] Kopiert. Nicht in einen Remote-Desktop eingefügt. Drücken Sie ⌘V zum Einfügen.
        [virtual-machine] Kopiert. Nicht in eine virtuelle Maschine eingefügt. Drücken Sie ⌘V zum Einfügen.
        [secure-field] Kopiert. Dieses Feld verbirgt Ihre Eingabe. Drücken Sie ⌘V zum Einfügen.
        [secure-input] Kopiert. Sichere Tastatureingabe ist aktiv, eventuell in einer anderen App. Drücken Sie ⌘V zum Einfügen.
        [elevated-app] Kopiert. Nicht in eine Administrator-App eingefügt. Drücken Sie ⌘V zum Einfügen.
        [key-release-failed] Kopiert. Ibis konnte die gedrückten Tasten nicht loslassen. Drücken Sie Ctrl und V und lassen Sie beide los, um einzufügen.
        [selection-not-shown] Kopiert. Ibis ersetzt nur Text, den es Ihnen gezeigt hat. Drücken Sie ⌘V, um ihn zu ersetzen.
        [citation-not-selected] Kopiert. Ibis ersetzt eine Zitation nur, wenn sie ausgewählt ist. Wählen Sie sie aus und drücken Sie dann ⌘V.
       *[paste-unavailable] Kopiert. Ibis konnte nicht bestätigen, wo der Text landen würde. Drücken Sie ⌘V zum Einfügen.
    }
update-title = Ibis wird aktualisiert
app-newer-installed = Eine neuere Ibis-Version ist installiert. Beenden Sie Ibis und öffnen Sie es erneut, um sie zu verwenden.
update-downloading = Wird heruntergeladen
update-installing = Wird geprüft und installiert
update-failed = Update nicht installiert. Siehe Einstellungen › Über Ibis.
library-entries-to-check =
    { $count ->
        [one] 1 Eintrag zu prüfen
       *[other] { $count } Einträge zu prüfen
    }
library-more-not-listed =
    { $count ->
        [one] 1 weiterer ist nicht aufgeführt.
       *[other] { $count } weitere sind nicht aufgeführt.
    }

citations-documents = In Word, LibreOffice und anderen Dokumenten
citations-keys = In LaTeX, Markdown und Obsidian
citations-keys-description = Wird verwendet, wo Sie Zitierschlüssel schreiben, und in Apps, die Ibis nicht kennt.
citations-apps = Für Apps gemerkte Formate
citations-forget = Vergessen
citations-writings = Projekte
settings-open-file = Einstellungsdatei öffnen
settings-open = Öffnen
settings-file-description = Änderungen an settings.json gelten, während Ibis läuft.
picker-use-usual = { $format } in { $app } wieder verwenden
picker-stop-writing = { $name } in { $app } nicht mehr verwenden

copied-notice-suggested = In { $format } eingefügt
copied-notice-suggestion = Vorgeschlagen: { $format }
copied-notice-change = Ändern

picker-find-open = PDF öffnen oder Eintrag in Zotero zeigen
shortcuts-action-find-open = PDF öffnen oder Eintrag zeigen

citations-unlisted = Eigenes Format

picker-format-suggestion = Vorgeschlagen, bis Sie ein Format wählen.
## The palette: the format chooser and More (ui/Palette).
palette-back = Zurück
palette-run = Ausführen
palette-recent = Zuletzt verwendet
palette-citation = Diese Zitation
palette-library = Diese Bibliothek
palette-ibis = Ibis
palette-peek = Kurzansicht der Referenz
palette-actions-title = Aktionen
palette-search-actions = Aktionen suchen
palette-no-actions = Keine passenden Aktionen
picker-format-title = Format
palette-search-formats = Formate suchen
palette-no-formats = Keine passenden Formate
palette-choose = Auswählen
palette-no-matches = Keine Treffer
palette-use-again = { $format } in { $app } wieder verwenden
palette-use-for-app = Für { $app } verwenden
palette-use-default = Überall verwenden
palette-use-this-paste = Nur dieses Mal verwenden
palette-style-unavailable = Gerade nicht verfügbar
palette-preview-example = Zum Beispiel
palette-preview-key = Was Ibis einfügt
palette-preview-more = und { $count } weitere
palette-preview-unavailable = Keine Vorschau für dieses Format.
palette-words-source = öffnen datei ordner zotero anzeigen bibliothek
palette-words-reveal-source = finder explorer ordner datei speicherort
palette-words-choose-source = bibliothek datei bib zotero öffnen wechseln ändern
palette-words-project-entry = literaturverzeichnis bib eintrag fehlt kopieren
palette-words-project-choose = literaturverzeichnis bib prüfen projekt
palette-words-project-stop = literaturverzeichnis bib prüfung beenden
palette-words-format = stil ausgabe apa pandoc latex syntax teilen exportieren bibtex doi json
palette-words-share = teilen exportieren senden kopieren referenzen
palette-words-peek = details vorschau kurzansicht info
palette-words-change-last = bearbeiten ersetzen vorherige zitation erneut
palette-words-reference-list = literaturverzeichnis literaturliste referenzen zitierte werke liste einträge einfügen kopieren
palette-words-citation-focus = fokus token referenz zitation
palette-words-citation-edit = bearbeiten seite fundstelle details zitation
palette-words-citation-move-left = verschieben anordnen vorherige token referenz zitation
palette-words-citation-move-right = verschieben anordnen nächste token referenz zitation
palette-words-citation-remove = entfernen löschen token referenz zitation
palette-words-keep-order = reihenfolge sortieren abfolge
palette-words-session-references = literaturverzeichnis verlauf zitiert liste referenzen
palette-words-forget-recent = löschen verlauf datenschutz zuletzt
palette-words-pin = anheften schwebend bleiben offen halten
palette-words-keys = tastaturkürzel tasten hilfe tastenkürzel
palette-words-settings = einstellungen optionen konfigurieren
palette-words-quit = beenden schließen stoppen
reason-strict-citation = Der strikte Modus verlangt eine vollständige Zitation ohne Verluste oder Quellenprobleme.
    .cli = Prüfen Sie die gemeldete Quelle oder die Zitationsoptionen, oder wiederholen Sie ohne --strict, um die Befunde zu prüfen.
    .agent = Beheben Sie das Problem in der Quelle oder den Zitationsoptionen, oder bitten Sie die Person, die Befunde einer normalen Zitation zu prüfen.
    .app = Prüfen Sie die Quelle und die Zitationsoptionen, bevor Sie es erneut versuchen.

cli-strict-help = Zitationen mit Verlusten, Zurückweisungen oder Quellenproblemen ablehnen
mcp-strict-help = Verluste, Zurückweisungen oder Quellenprobleme ablehnen.
mcp-citation-hidden = Diese Zitation enthält verborgene Zeichen und kann nicht unverändert zurückgegeben werden. Bitten Sie die Person, die Quelle zu korrigieren oder eine andere Ausgabe zu wählen.
mcp-citation-source-failed = Eine Quelle konnte nicht aktualisiert werden; diese Zitation verwendet ihre letzte gültige Kopie. Der Beleg vermerkt den letzten erfolgreichen Lesevorgang.
mcp-citation-source-warnings = Eine zitierte Quelle hat Warnungen; ihr Beleg vermerkt deren Anzahl.
mcp-cite-description = Zitiert refs aus search der Reihe nach, optional mit Fundstellen. Verwenden Sie den zurückgegebenen Text unverändert und informieren Sie die Person über Hinweise, Verluste und Warnungen.

# The desktop's bundled command. Never execute another program to check its version.
command-install = ibis-Befehl installieren
command-install-button = Installieren
command-install-description = Im Terminal suchen und zitieren, mit derselben Bibliothek und denselben Einstellungen.
command-remove = Befehl entfernen
command-checking = ibis-Befehl wird geprüft…
command-missing = Nicht installiert
command-matching = ibis { $version } – passt zu dieser App
command-different = ibis { $version } – eine andere Version als diese App
command-foreign = Ein anderes ibis wird verwendet: { $path }. Version nicht verfügbar.
command-profile = Fügen Sie diese Zeile Ihrem Shell-Profil hinzu und öffnen Sie das Terminal neu. Ibis prüft den PATH, den die App sieht.
command-copy = Kopieren
command-copied = Kopiert
command-failed = Der ibis-Befehl konnte nicht geändert werden. Prüfen Sie die Ordnerberechtigungen und versuchen Sie es erneut.

# Source status and reading warnings; separate from citation warnings.
source-warning-zotero-newer-than-tested = Zotero ist neuer als die Version, mit der diese Ibis-Version getestet wurde. Die Ergebnisse sehen richtig aus, aktualisieren Sie Ibis aber, sobald Sie können.
source-warning-cloud-synced-directory = Die Bibliothek liegt in einem Cloud-synchronisierten Ordner, daher können Änderungen verspätet ankommen.
source-warning-citation-keys-unavailable = Better-BibTeX-Schlüssel konnten nicht gelesen werden, daher fehlen sie bei einigen Einträgen.
source-warning-syntax = Ein Eintrag konnte nicht gelesen werden, daher überspringt Ibis ihn.
source-warning-duplicate-key = Mehrere Einträge verwenden denselben Schlüssel oder dieselbe ID, daher überspringt Ibis sie alle.
source-warning-field = Ein Feld konnte nicht vollständig umgewandelt werden; prüfen Sie diesen Eintrag.
source-warning-work-limit = Die Bibliothek ist zu groß, um sie vollständig zu lesen.
source-warning-invalid-attachment = Ein Anhangslink ist ungültig.
source-warning-pdfs-not-kept = Dieser Ordner enthält PDFs, die Ibis noch nicht aufgenommen hat.

# Per-setting reset actions name their setting for assistive technology.
settings-use-default = Standard für { $setting } verwenden
settings-default-restored = Standard für { $setting } wiederhergestellt
settings-undo-default = Standard für { $setting } wiederherstellen
settings-details = Details…
settings-shortcut-words = tastatur tastenkürzel tastenbelegung kurzbefehl

## Citation diagnostics carried to clients before rendering.
cli-ambiguous-suggestion = Dieser Schlüssel ist mehrdeutig. Wählen Sie die id eines Ergebnisses dieser Suche und verwenden Sie dann ibis show --id ID_JSON mit denselben Bibliotheken.
cli-ambiguous-search-keys = Hinweis: Treffer mit mehrdeutigen Zitierschlüsseln wurden ausgelassen ({ $count }); suchen Sie mit --json und verwenden Sie die id des Ergebnisses.
problem-empty-selection = Es gibt nichts zu zitieren.
problem-too-many-items = Eine Zitation kann höchstens { $count } Referenzen enthalten.
problem-invalid-locator = Korrigieren Sie die Seite oder Fundstelle dieser Referenz.
    .named = Die Seite oder Fundstelle für { $reference } ist leer oder enthält Steuerzeichen.
problem-invalid-rich-text = Korrigieren Sie den Text vor oder nach dieser Referenz.
    .named = Der Text vor oder nach { $reference } enthält Zeichen, die Ibis nicht schreiben kann.
problem-output-too-large = Die Zitation ist zu groß zum Schreiben.
problem-unsupported-form = Diese Ausgabe kann die gewählte Zitationsform nicht schreiben.
    .app-named = { $reference }: { $form } lässt sich in dieser Ausgabe oder Gruppe nicht darstellen.
    .cli-form = { $reference } kann in { $output } nicht mit --form { $form } zitiert werden. Versuchen Sie --form normal.
    .cli-options = { $reference } kann mit { $options } nicht in { $output } zitiert werden. Versuchen Sie es ohne.
    .named = { $output } kann { $reference } nicht in dieser Form oder mit diesen Optionen schreiben.
    .cli = Zitieren Sie sie einzeln oder wählen Sie eine andere Ausgabe.
    .agent = Versuchen Sie die Form normal oder ein anderes Format bzw. einen anderen Stil.
    .app = Wählen Sie „Name in Klammern“, zitieren Sie sie einzeln oder wählen Sie eine andere Ausgabe.
    .bibliography = Entfernen Sie die Referenzen, die dieser Stil nicht schreiben kann, aus dieser Liste.
problem-missing-item = Diese Referenz wurde während des Zitierens entfernt.
    .named = { $reference } wurde während des Zitierens geändert oder entfernt.
    .cli = Versuchen Sie es erneut.
    .agent = Suchen Sie erneut.
    .app = Suchen Sie erneut.
problem-changed-item = Diese Referenz wurde während des Zitierens geändert.
    .named = { $reference } wurde während des Zitierens geändert.
    .cli = Versuchen Sie es erneut.
    .agent = Suchen Sie erneut.
    .app = Suchen Sie erneut.
problem-missing-key = Diese Referenz hat keinen Zitierschlüssel.
    .named = { $reference } hat keinen Zitierschlüssel.
    .cli = Zitieren Sie sie stattdessen mit --style.
    .agent = Eine Schlüsselsyntax kann sie nicht benennen; verwenden Sie einen Stil.
    .app = Vergeben Sie in Ihrer Bibliothek einen, oder wählen Sie einen Stil.
problem-missing-details = Diese Referenz hat noch keinen Titel, keine Autorschaft und keine Herausgabe, daher lässt sie sich nicht zitieren. Ergänzen Sie diese Angaben in ihrer Bibliothek.
    .named = { $reference } hat noch keinen Titel, keine Autorschaft und keine Herausgabe.
    .cli = Ergänzen Sie in ihrem Eintrag einen Titel oder eine Autorschaft und zitieren Sie erneut.
    .agent = Sie lässt sich erst zitieren, wenn ihre Bibliothek einen Titel, eine Autorschaft oder eine Herausgabe enthält.
    .app = Ergänzen Sie in ihrer Bibliothek einen Titel oder eine Autorschaft.

problem-ambiguous-key = Mehrere Referenzen teilen sich diesen Zitierschlüssel.
    .named = Mehrere Referenzen teilen sich den Zitierschlüssel von { $reference }.
    .cli = Geben Sie in Ihrer Bibliothek jeder einen eigenen Schlüssel.
    .agent = Verwenden Sie einen Stil, oder bitten Sie die Person, jeder einen eigenen Schlüssel zu geben.
    .app = Geben Sie in Ihrer Bibliothek jeder einen eigenen Schlüssel.
problem-conflicting-key = Diese Referenz hat widersprüchliche Zitierschlüssel.
    .named = { $reference } hat in ihrer Bibliothek mehr als einen Zitierschlüssel.
    .cli = Behalten Sie einen.
    .agent = Verwenden Sie einen Stil, oder bitten Sie die Person, nur einen Schlüssel zu behalten.
    .app = Behalten Sie einen in Ihrer Bibliothek, oder wählen Sie einen Stil.
problem-invalid-key = Diese Ausgabe kann den Zitierschlüssel der Referenz nicht schreiben.
    .named = Der Zitierschlüssel von { $reference } enthält Zeichen, die { $output } nicht schreiben kann.
    .agent = Verwenden Sie einen Stil, oder bitten Sie die Person, den Schlüssel zu ändern.
problem-unsupported-output = Diese Zitationsausgabe ist nicht verfügbar.
    .named = Es gibt keinen Stil { $output }.
    .cli = Stile auflisten mit: ibis styles
    .agent = Das Tool styles listet sie auf.
problem-invalid-locale = Diese Zitiersprache ist nicht verfügbar.
    .named = Diese Zitiersprache ist nicht verfügbar.
    .cli = Auflisten mit: ibis styles --locales
    .agent = Das Tool styles listet die Sprachen auf.
problem-cancelled = Die Zitation wurde abgebrochen.
problem-formatter-unavailable = Die Zitation konnte gerade nicht geschrieben werden.
    .named = Die Zitation konnte gerade nicht geschrieben werden. Versuchen Sie es erneut.
problem-annotation-mismatch = Diese Anmerkung gehört zu einer anderen Referenz.
    .named = Die für { $reference } angegebene Anmerkung gehört zu einer anderen Referenz.
problem-not-citable = Dieses Objekt kann nicht als Referenz zitiert werden.
    .named = { $reference } ist keine Referenz (etwa eine Notiz) und kann daher nicht zitiert werden.
problem-not-from-zotero = Diese Referenz wurde nicht aus Zotero gelesen.
    .named = { $reference } wurde nicht aus einer Zotero-Bibliothek gelesen, daher kann Zotero sie nicht finden.
    .cli = Lesen Sie Ihre Zotero-Bibliothek mit --zotero, oder wählen Sie eine andere Ausgabe.
    .agent = Verwenden Sie ein anderes Format, oder bitten Sie die Person, sie aus ihrer Zotero-Bibliothek zu zitieren.
    .app = Zitieren Sie sie aus Ihrer Zotero-Bibliothek, oder wählen Sie eine andere Ausgabe.
problem-missing-doi = Eine ausgewählte Referenz hat keine verwendbare DOI.
    .named = { $reference } hat keine verwendbare DOI.
    .cli = Teilen Sie sie stattdessen mit --format bibtex.
    .agent = Verwenden Sie stattdessen format bibtex.
    .app = Wählen Sie BibTeX im Formatmenü.

## Native command failures.
app-failure-cli-status = Die Einrichtung der Befehlszeile konnte nicht gelesen werden.
app-failure-set-cli-installed = Die Einrichtung der Befehlszeile konnte nicht geändert werden.
app-failure-copy-cli-profile = Die Shell-Einrichtung konnte nicht kopiert werden.
app-failure-hide-picker = Das Fenster konnte nicht geschlossen werden.
app-failure-copy-editor-endpoint = Die Editor-Adresse konnte nicht kopiert werden.
app-failure-copy-version = Die Version konnte nicht kopiert werden.
app-failure-set-window-size = Die Fenstergröße konnte nicht gespeichert werden.
app-failure-undo-window-position = Die Fensterposition konnte nicht wiederhergestellt werden.
app-failure-copy-project-entry = Der Referenzeintrag konnte nicht kopiert werden.
app-failure-finish-paste-guide = Die Einrichtung des Einfügens konnte nicht abgeschlossen werden.
app-failure-row-context = Die Referenzdetails konnten nicht gelesen werden.
app-failure-find-zotero = Zotero konnte nicht geprüft werden.
app-failure-find-sources = Die Suche nach Bibliotheken ist fehlgeschlagen.
app-failure-open-found-library = Diese Bibliothek konnte nicht geöffnet werden.
app-failure-configure-shortcuts = Die Einrichtung der Tastaturkürzel konnte nicht geöffnet werden.
app-failure-use-sample-library = Die Beispielreferenzen konnten nicht geöffnet werden.
app-failure-use-zotero-libraries = Wählen Sie mindestens eine Zotero-Bibliothek, ohne Wiederholungen.
app-failure-recent-references = Die zuletzt verwendeten Referenzen konnten nicht gelesen werden.
app-failure-search = Die Suche konnte nicht abgeschlossen werden.
app-failure-drag-paste-app = Die Ibis-App konnte nicht gezogen werden.

app-command-no-library = Wählen Sie eine Bibliothek zum Durchsuchen.
app-project-error-invalid-name = Geben Sie dem Projekt einen Namen mit 1 bis 80 Zeichen, ohne Steuerzeichen.
app-project-error-duplicate-name = Ein Projekt mit diesem Namen gibt es schon. Wählen Sie einen anderen Namen.
app-project-error-not-found = Dieses Projekt ist nicht mehr verfügbar. Wählen Sie erneut ein Projekt.
app-project-error-too-many = Ibis kann sich höchstens 64 Projekte merken. Entfernen Sie eines, bevor Sie ein weiteres hinzufügen.
app-project-error-invalid = Die Definitionen der Projekte sind ungültig. Prüfen Sie die Einstellungen.
app-project-error-changed = Projekt oder Ziel haben sich geändert. Öffnen Sie das Formatmenü erneut.
app-project-error-editor = Diese Editor-Anfrage wählt ihre eigene Ausgabe. Schließen Sie sie ab, bevor Sie das Projekt wechseln.
app-project-error-output = Das Format dieses Projekts ist im Fenster nicht verfügbar. Wählen Sie ein anderes Format.
app-settings-error-unreadable = Die Einstellungsdatei konnte nicht gelesen oder gespeichert werden.
app-settings-error-invalid = Die Einstellungsdatei ist ungültig. Öffnen Sie die Einstellungen, um sie zu prüfen.
    .settings = settings.json ist ungültig, daher verwendet Ibis Ihre letzten gültigen Einstellungen.
app-settings-error-newer = Die Einstellungen stammen von einer neueren Ibis-Version. Aktualisieren Sie Ibis, bevor Sie sie ändern.
app-settings-error-read-only = Die Einstellungen sind schreibgeschützt. Machen Sie die Einstellungsdatei beschreibbar und versuchen Sie es erneut.
app-command-output-not-saved = Die Formatwahl konnte nicht gespeichert werden.
app-command-delivery-not-saved = Das Format wurde gespeichert, die Einfügeoption jedoch nicht.

app-command-page-missing = Referenz { $number }: { $label } braucht eine Seite oder einen Bereich. Geben Sie sie nach der Bezeichnung ein, oder entfernen Sie { $label }.
app-command-page-invalid = Referenz { $number }: { $label } braucht nach der Bezeichnung eine gültige Seite oder einen gültigen Bereich. Korrigieren Sie sie, oder entfernen Sie { $label }.
app-command-locator-missing = Referenz { $number }: { $label } braucht einen Wert. Geben Sie ihn nach der Bezeichnung ein, oder entfernen Sie die Fundstelle.
app-command-locator-invalid = Referenz { $number }: { $label } braucht einen gültigen Wert. Korrigieren Sie ihn, oder entfernen Sie die Fundstelle.
app-reference-number = Referenz { $number }
app-command-locator-conflict = Referenz { $number }: Eine eingegebene und eine übernommene Fundstelle lassen sich nicht kombinieren.
app-command-bibliography-unsupported = Dieser Stil kann diese Referenzen nicht schreiben: { $references }. Entfernen Sie sie aus dieser Liste, um fortzufahren.

## MCP validation facts.
mcp-error-arguments = arguments muss ein Objekt sein.
mcp-error-query = query muss aus wenigen Wörtern bestehen: einem Autorennachnamen, Titelwörtern, einem Jahr oder einem Schlüssel.
mcp-error-shorthand = shorthand muss true oder false sein.
mcp-error-reference = ref muss eine ref aus search sein, als Text.
mcp-error-items = items muss eine Liste von Objekten sein, jedes mit einer ref aus search.
mcp-error-app = for_app muss eine macOS-Bundle-ID oder ein Windows-Programmname sein.
mcp-error-keep-order = keep_order muss true oder false sein.
mcp-error-output-choice = Geben Sie entweder format oder style an, nicht beides.
mcp-error-style = style muss eine Stil-ID sein, als Text.
mcp-error-locale = locale muss ein Sprach-Tag sein, als Text, etwa de-DE.
mcp-error-key-locale = locale gilt nur für einen Stil; bei einer Schlüsselsyntax bestimmt der Prozessor des Dokuments die Sprache.
mcp-error-form = form muss einer der Werte im Eingabeschema dieses Tools sein.
mcp-error-records-or-items = Geben Sie records oder items an, nicht beides; for_app braucht items.
mcp-error-records-order = keep_order braucht items; mit records wird es nicht unterstützt.
mcp-error-records = records muss eine Liste von CSL-JSON-Objekten sein, jedes mit id und type.
mcp-error-item = Jedes item muss ein Objekt mit einer ref aus search sein.
mcp-error-locator = Die Fundstelle ist nicht lesbar; schreiben Sie sie etwa als „p. 12“, „ch. 3“ oder „12-14“.
mcp-error-omit-author = omit_author muss true oder false sein.
mcp-error-empty-output = Diese Ausgabe hat für diese items keinen Text erzeugt; versuchen Sie eine andere Form, ein anderes Format oder einen anderen Stil.
mcp-error-style-query = query muss Text von höchstens 1024 Bytes sein.
mcp-error-limit = limit muss eine ganze Zahl ab 1 sein; es werden höchstens { $limit } Ergebnisse zurückgegeben.
mcp-error-unknown-argument = { $tool } hat kein Argument '{ $name }'; es akzeptiert: { $names }.
mcp-error-format = format muss einer dieser Werte sein: { $formats }.
mcp-error-text = { $name } muss Text sein.
mcp-error-number-range = { $name } muss eine ganze Zahl von 1 bis { $limit } sein.
mcp-error-number-minimum = { $name } muss eine ganze Zahl ab 0 sein.
mcp-error-include-receipt = include_receipt muss true oder false sein.
mcp-error-stale-reference = Diese Referenz hat sich geändert, seit search sie geliefert hat; suchen Sie erneut und prüfen Sie, ob es die gemeinte ist.
mcp-error-attachment-text = Die Person hat KI-Apps nicht erlaubt, Anhangstext aus dieser Bibliothek zu lesen. Sagen Sie es ihr; sie kann es für diese Bibliothek in den Ibis-Einstellungen erlauben.
mcp-error-no-library = Keine Bibliothek ist geöffnet. Bitten Sie die Person, in Ibis eine Bibliothek zu wählen oder `ibis mcp` mit --file oder --zotero zu starten, und starten Sie dann eine neue Sitzung.
mcp-error-writes = Dieser Server wurde ohne --allow-writes gestartet und kann daher keine Änderungen vorschlagen. Sagen Sie es der Person; sie kann --allow-writes zum Befehl ibis mcp in ihrer KI-App hinzufügen.
mcp-error-proposal = Ibis hat diesen Vorschlag nicht erfasst: { $reason }
mcp-error-strict = { mcp-strict-help }
mcp-error-hidden-citation = { mcp-citation-hidden }
mcp-item-error = Element { $number }: { $reason }
mcp-item-name = Element { $number }
mcp-output-name = Diese Ausgabe
mcp-tool-error = Fehler: { $code }. { $reason }
mcp-error-busy = Ibis ist beschäftigt. Wiederholen Sie diesen Aufruf gleich.

cli-key-suggestions = Meinten Sie { $keys }?
cli-key-not-found = keine Referenz hat den Schlüssel '{ $key }'{ $choices }
    Versuchen Sie: { $command }{ $route }

mcp-startup-failed = Die Bibliothek konnte beim Start dieser Sitzung nicht geöffnet werden. { $error } Beheben Sie das Problem und starten Sie dann eine neue MCP-Sitzung.

app-bibliography-unavailable = Das Literaturverzeichnis konnte nicht erstellt werden. Versuchen Sie es erneut.
app-bibliography-not-saved = Das Literaturverzeichnis konnte nicht gespeichert werden. Versuchen Sie es erneut.
app-bibliography-name-taken = Eine Datei mit diesem Namen gibt es dort schon. Wählen Sie einen anderen Namen.
app-bibliography-format = Speichern Sie es als .rtf, .html oder .txt.
app-clipboard-unavailable = In die Zwischenablage konnte nicht geschrieben werden. Versuchen Sie es erneut.

app-recent-history-not-saved = Die letzten Zitationen konnten nicht vergessen werden. Versuchen Sie es erneut.
app-project-citation-conflict = Die Zitierschlüssel { $keys } bezeichnen im Literaturverzeichnis unterschiedliche Werke. Prüfen Sie das Literaturverzeichnis vor dem Kopieren.

app-settings-error-changed = Die Einstellungen haben sich beim Speichern geändert. Versuchen Sie es erneut.
app-settings-error-busy = Ein anderes Ibis speichert gerade die Einstellungen. Warten Sie, bis es fertig ist, und versuchen Sie es dann erneut.
app-settings-error-document-format = Wählen Sie einen Stil für Dokumente.
app-settings-error-key-format = Wählen Sie ein Schlüsselformat für LaTeX, Markdown und Obsidian.
app-project-error-context-unavailable = Literaturverzeichnis und Zitierstil des Dokuments konnten nicht gelesen werden. Versuchen Sie es erneut.
app-project-error-invalid-context = Geben Sie im Dokument genau ein Literaturverzeichnis und einen CSL-Stil als einfache Werte an.
app-project-error-ambiguous-context = Mehrere Projekte passen zu diesem Literaturverzeichnis und Zitierstil. Wählen Sie ein Projekt nach Namen.
mcp-error-citation-format = Dieses Format exportiert Datensätze oder Zotero-Marker. Wählen Sie ein Zitationsformat.

app-failure-open-settings-file = Die Einstellungsdatei konnte nicht geöffnet werden. Versuchen Sie es erneut.
## Command-line requests, diagnostics and reports.

cli-keys-unreadable = die Schlüsselliste konnte nicht gelesen werden; geben Sie UTF-8-Text mit einem Schlüssel pro Zeile an.
cli-keys-too-large = die Schlüsselliste ist größer als 1 MiB; senden Sie weniger Schlüssel auf einmal.
cli-keys-not-utf8 = die Schlüsselliste muss UTF-8-Text mit einem Schlüssel pro Zeile sein.
cli-keys-empty = die Schlüsselliste ist leer; geben Sie mindestens einen Zitierschlüssel an.
cli-key-file-unavailable = die Schlüsseldatei fehlt, ist nicht lesbar oder keine reguläre Datei; prüfen Sie den Pfad, oder verwenden Sie - für stdin.
cli-key-file-unopened = die Schlüsseldatei konnte nicht geöffnet werden; prüfen Sie Pfad und Berechtigungen.
cli-key-file-not-regular = --keys-from braucht eine reguläre Datei oder - für stdin.
cli-key-file-timeout = die Schlüsseldatei konnte nicht rechtzeitig gelesen werden; versuchen Sie es erneut.
cli-toggle-unprepared = das Umschalten konnte nicht vorbereitet werden; prüfen Sie, ob der temporäre Ordner beschreibbar ist, und versuchen Sie es erneut.
cli-app-libraries-unavailable = --app-libraries ist hier nicht verfügbar; nennen Sie die zu lesenden Bibliotheken.
cli-records-with-libraries = --records zitiert nur die Datensätze aus stdin; lassen Sie die Bibliotheksoptionen weg.
cli-search-words-needed = geben Sie Suchwörter an: einen Autor, Wörter aus dem Titel, ein Jahr oder einen Schlüssel.
cli-search-words-too-long = die Suchwörter sind zu lang; verwenden Sie weniger.
cli-id-not-found = keine geöffnete Bibliothek enthält die Referenz, die --id nennt.
cli-attachment-not-found = keine geöffnete Bibliothek enthält diesen Anhang, oder seine Referenz hat sich geändert; führen Sie ibis outline erneut aus, um eine neue ID zu erhalten.
cli-find-in-words = geben Sie die zu suchenden Wörter an, bis zu 200 Zeichen.
cli-records-too-large = die Datensätze aus stdin sind größer als 1 MiB; senden Sie weniger auf einmal.
cli-bibliography-needs-style = --bibliography braucht einen Stil: geben Sie --style an oder ein --format, das ein Stil ist.
cli-project-too-many-keys = es können höchstens 1000 Schlüssel auf einmal geprüft werden.
cli-styles-limit = --limit muss zwischen 1 und 20000 liegen.
cli-request-help = stdin muss genau ein CitationRequest-JSON-Objekt enthalten; siehe: ibis schema cite
cli-records-help = stdin muss genau ein JSON-Array mit CSL-JSON-Datensätzen enthalten; siehe: ibis schema records
cli-id-help = --id muss die ID einer Referenz sein, also das JSON, das ibis search --json ausgibt.
cli-key-option-owner = --{ $flag } muss auf den Schlüssel folgen, zu dem es gehört.
cli-key-option-repeated = --{ $flag } ist für „{ $key }“ zweimal angegeben.
cli-locator-unreadable = --locator „{ $locator }“ ist keine Fundstelle, die Ibis lesen kann; schreiben Sie sie etwa als „p. 12“, „ch. 3“ oder „12-14“.
cli-method-unknown = es gibt keine Methode „{ $method }“; wählen Sie eine aus: { $names }
cli-style-unknown = es gibt keinen Stil „{ $style }“; Stile auflisten mit: ibis styles
cli-locale-unknown = es gibt keine Zitiersprache „{ $locale }“; auflisten mit: ibis styles --locales
cli-params-help = --params muss die JSON-Parameter von { $method } enthalten; siehe: ibis schema { $method }
cli-source-findings = Hinweis: eine Bibliothek hat Warnungen oder konnte nicht aktualisiert werden; siehe: ibis libraries{ $libraries }{ $json }
cli-keys-waiting = warte auf Schlüssel, einer pro Zeile; mit Ctrl-D beenden.
cli-search-keyless =
    { $plural ->
        [one] Hinweis: { $count } Treffer hat keinen Zitierschlüssel; siehe ibis search { $query }{ $libraries } --json
       *[other] Hinweis: { $count } Treffer haben keinen Zitierschlüssel; siehe ibis search { $query }{ $libraries } --json
    }
cli-search-more =
    { $plural ->
        [one] Hinweis: der erste { $count } Treffer wird angezeigt; mehr mit --limit { $next }
       *[other] Hinweis: die ersten { $count } Treffer werden angezeigt; mehr mit --limit { $next }
    }
cli-search-narrow = Hinweis: die ersten { $count } Treffer werden angezeigt; verwenden Sie genauere Suchwörter
cli-file-name-invalid = { $path } kann nicht geöffnet werden: der Name ist kein gültiger Text; benennen Sie die Datei um.
-cli-name-library = geben Sie eine mit --file PATH, --zotero oder --zotero-group ID an
cli-app-settings-timeout = die Einstellungen der Ibis-App konnten nicht rechtzeitig gelesen werden; versuchen Sie es erneut, oder { -cli-name-library }.
cli-chosen-library-unopened = die in { $app } gewählte Bibliothek kann nicht geöffnet werden: { $reason } Wählen Sie sie dort erneut, oder { -cli-name-library }.
cli-app-settings-missing = keine Bibliothek angegeben, und die Ibis-App hat hier keine Einstellungen; { -cli-name-library }.
cli-app-formats-unreadable = die in { $app } gewählten Formate können nicht gelesen werden: { $reason }. Öffnen Sie { $app }, um die Einstellungen zu reparieren.
cli-app-libraries-unreadable = die in { $app } gewählten Bibliotheken können nicht gelesen werden: { $reason }. { $fix }, oder { -cli-name-library }.
cli-app-library-unchosen = in { $app } ist noch keine Bibliothek gewählt; wählen Sie dort eine, oder { -cli-name-library }.
cli-app-library-relative = die in { $app } gewählte Bibliothek ist kein vollständiger Pfad; wählen Sie sie dort erneut, oder { -cli-name-library }.
cli-app-settings-changed = die Einstellungen haben sich beim Speichern geändert; versuchen Sie es erneut
cli-app-settings-permission = dieses Programm darf die Einstellungsdatei nicht lesen; erlauben Sie es, oder führen Sie es außerhalb der Sandbox aus
cli-app-settings-file-missing = die Einstellungsdatei fehlt
cli-app-settings-unreadable = die Einstellungsdatei konnte nicht gelesen werden
cli-app-settings-invalid = die Einstellungsdatei ist ungültig
cli-app-settings-backup-unused = { $reason }, und Ibis verwendet die Sicherung hier nicht
cli-app-settings-update = Aktualisieren Sie diesen ibis-Befehl passend zu { $app }
cli-app-settings-repair = Öffnen Sie { $app }, um die Einstellungen zu reparieren
cli-app-settings-zotero-invalid = die Einstellungsdatei ist ungültig (Zotero-Bibliothekseintrag { $position })
cli-app-settings-newer = die Einstellungen stammen von einer neueren Ibis-Version (Format { $version })
cli-settings-no-folder = dieser Computer hat keinen Ordner für App-Einstellungen (HOME oder APPDATA ist nicht gesetzt).
cli-settings-action-needed = wählen Sie export oder import; siehe: ibis settings --help
cli-settings-backup-needed = nennen Sie die Sicherungsdatei, die wiederhergestellt werden soll.
cli-settings-folder-timeout = der Einstellungsordner hat nicht rechtzeitig reagiert; versuchen Sie es erneut.
cli-settings-exported = Einstellungen von { $app } in { $name } gesichert.
    Einfügefreigaben und Fensterpositionen bleiben auf diesem Computer.
cli-settings-backup-made = eine Sicherung von Ibis { $version } auf { $system }
cli-settings-unchanged = { $app } hat bereits die Einstellungen aus { $backup }. Nichts geändert.
cli-settings-would-restore = Das Wiederherstellen von { $backup } würde die Einstellungen von { $app } ändern:
cli-settings-restored = { $backup } in { $app } wiederhergestellt:
cli-settings-library-not-found = Hinweis: die Bibliotheksdatei aus der Sicherung ist nicht auf diesem Computer. Sie bleibt eingetragen, und Ibis findet sie, sobald die Datei da ist.
cli-settings-shortcut-kept = Hinweis: die Sicherung hat kein Tastenkürzel zum Öffnen für dieses System, daher bleibt das dieses Computers erhalten.
cli-settings-repairs-settings = Hinweis: die Einstellungsdatei auf diesem Computer ist beschädigt oder fehlt. Die Wiederherstellung schreibt sie neu und behält eine beschädigte Datei daneben.
cli-settings-dry-run = Nichts geändert. Führen Sie den Befehl ohne --dry-run erneut aus, um wiederherzustellen.
cli-settings-restored-next = Die ersetzten Einstellungen bleiben als settings.json.bak erhalten. Einfügefreigaben und Fensterpositionen bleiben unverändert.
    Ibis verwendet die wiederhergestellten Einstellungen beim nächsten Öffnen; beenden und öffnen Sie Ibis neu, falls es läuft.
cli-writing-style-unavailable = Dieser Zitierstil ist nicht verfügbar. Stile auflisten mit: ibis styles
cli-writing-output-unavailable = Diese Ausgabe ist nicht verfügbar. Ausgaben auflisten mit: ibis outputs
cli-writing-bibliography-needed = Wählen Sie ein BibTeX- oder BibLaTeX-Literaturverzeichnis mit einem .bib-Dateinamen.
cli-writing-folder-unavailable = Der aktuelle Ordner ist nicht verfügbar. Geben Sie den vollständigen Pfad des Literaturverzeichnisses an.
cli-writing-action-needed = Wählen Sie list, create, rename, update oder remove.
cli-writings-empty = Noch keine Projekte. Erstellen Sie eines mit: ibis projects create NAME --format pandoc
cli-writing-format-needed = Wählen Sie --format oder --style.
cli-writing-citation-unavailable = Die Zitation des Projekts ist nicht verfügbar.
cli-writing-saved =
    { $action ->
        [create] Projekt { $name } erstellt.
        [rename] Projekt { $name } umbenannt.
        [remove] Projekt { $name } entfernt. Seine .bib-Datei bleibt erhalten.
       *[update] Projekt { $name } aktualisiert.
    }
cli-no-library = Keine Bibliothek geöffnet; fügen Sie --file <library> oder --zotero hinzu.
cli-search-no-match-terminal = Keine passenden Referenzen. Versuchen Sie eine kürzere Suche.
cli-search-no-match = Keine Referenzen gefunden.
cli-libraries-empty = Keine Bibliotheken geladen. Öffnen Sie eine mit --file <library>.
cli-styles-no-match = Kein Stil passt zu diesen Wörtern.
cli-share-written =
    { $count ->
        [one] 1 Datei, { $size }, nach { $path } geschrieben.
       *[other] { $count } Dateien, { $size }, nach { $path } geschrieben.
    }
cli-share-preview =
    { $count ->
        [one] 1 Datei, { $size }. Fügen Sie --to PATH hinzu, um das Paket zu schreiben.
       *[other] { $count } Dateien, { $size }. Fügen Sie --to PATH hinzu, um das Paket zu schreiben.
    }
cli-share-not-in-list = die Leseliste lässt aus, was ihr Stil nicht schreiben kann: { $keys }
cli-share-not-in-bibtex = References.bib lässt aus, was keinen brauchbaren Zitierschlüssel hat: { $keys }
cli-share-without-pdf = für diese konnte kein PDF aus ihrer Bibliothek hinzugefügt werden: { $keys }
cli-outline-none = Diese Referenz hat keine Anhänge.
cli-outline-pages = Text, { $count } Seiten
cli-outline-printed-pages = Text, { $count } Seiten mit gedruckten Seitenzahlen
cli-outline-text = Text, Seitenzahl unbekannt
cli-outline-not-indexed = kein Text: Zotero hat die Datei nicht indiziert
cli-outline-no-file = kein Text: ein Link ohne Datei
cli-outline-too-large = kein Text: größer, als Ibis liest
cli-outline-not-yet = kein Text: Ibis kann diese Datei noch nicht lesen
cli-outline-missing = kein Text: keine Datei dort, wo die Bibliothek sie angibt
cli-outline-not-local = kein Text: keine normale Datei auf diesem Computer, etwa ein Link oder eine reine Online-Datei
cli-outline-not-pdf = kein Text: die Datei ist kein PDF
cli-outline-damaged = kein Text: das PDF ist beschädigt
cli-outline-timed-out = kein Text: das Lesen des PDF dauerte länger als zehn Sekunden
cli-read-page = [Dateiseite { $page }]
cli-read-printed-page = [Dateiseite { $page }, gedruckte Seite { $label }]
cli-read-more = Weiterer Text: { $command }
cli-find-in-none = Keine Passage enthält diese Wörter.
cli-find-in-more = Die ersten { $count } Passagen; fügen Sie Wörter hinzu, um einzugrenzen.
cli-entry-no-original = Diese Referenz hat hier keinen ursprünglichen BibTeX-Eintrag: Nur eine .bib-Bibliothek bewahrt ihn auf, Zotero-, CSL-JSON-, RIS- und CSV-Bibliotheken nicht. Um einen Eintrag aus ihren Feldern zu schreiben, führen Sie ibis cite KEY --format bibtex aus, oder exportieren Sie sie aus Ihrer Literaturverwaltung im Dialekt Ihres Projekts und behalten Sie ihren Schlüssel bei.
cli-entry-changed = Die Bibliotheksdatei hat sich geändert oder konnte nicht gelesen werden. Laden Sie sie neu und versuchen Sie es erneut.
cli-entry-repeated = Die Bibliotheksdatei enthält diesen Schlüssel mehrfach, daher kann Ibis nicht erkennen, welcher Eintrag gemeint ist.
cli-entry-dependency = Dieser Eintrag hängt von crossref, xdata oder @string ab, was nicht mitgegeben werden kann. Exportieren Sie ihn aus Ihrer Literaturverwaltung und behalten Sie seinen Schlüssel bei.
cli-library-refresh-failed = Hinweis: diese Bibliothek konnte nicht aktualisiert werden; angezeigt wird der letzte erfolgreich gelesene Stand
cli-library-not-watched = Hinweis: Änderungen werden nicht automatisch übernommen
cli-warning-unknown-field = ein unbekanntes Feld wurde übersprungen; Ibis hat die Referenz behalten
cli-warning-unknown-type = der Eintragstyp wird nicht unterstützt; Ibis überspringt diesen Eintrag
cli-warning-duplicate-key = mehrere Einträge verwenden diesen Schlüssel oder diese ID; Ibis überspringt sie alle
cli-warning-syntax-entry = dieser Eintrag konnte nicht gelesen werden; Ibis überspringt ihn
cli-warning-entry-too-large = der Eintrag ist zu groß zum Lesen; Ibis überspringt ihn
cli-warning-library-incomplete = Ibis konnte die Bibliothek nicht vollständig lesen; einige Referenzen fehlen möglicherweise
cli-warning-attachment-skipped = ein Anhangslink ist ungültig; Ibis lässt ihn weg
cli-warning-syntax = ein Eintrag konnte nicht gelesen werden, daher überspringt Ibis ihn
cli-warning-field = ein Feld konnte nicht vollständig umgewandelt werden; prüfen Sie diesen Eintrag
cli-warning-work-limit = die Bibliothek ist zu groß, um sie vollständig zu lesen
cli-warning-invalid-attachment = ein Anhangslink ist ungültig
cli-warning-pdfs-not-kept = der Ordner enthält PDFs, die Ibis noch nicht aufgenommen hat; führen Sie ibis sources keep --file ORDNER aus
cli-warning-cloud-directory = die Bibliothek liegt in einem Cloud-synchronisierten Ordner, daher können Änderungen verspätet ankommen
cli-warning-keys-unavailable = Better-BibTeX-Schlüssel konnten nicht gelesen werden, daher fehlen sie bei einigen Einträgen
cli-warning-unknown = etwas in der Bibliothek sollte geprüft werden
cli-output-disk-full = die Ausgabe konnte nicht geschrieben werden, weil der Datenträger voll ist; geben Sie Speicherplatz frei und versuchen Sie es erneut.
cli-output-unwritten = die Ausgabe konnte nicht geschrieben werden; prüfen Sie das Ziel und versuchen Sie es erneut.
cli-file-busy = die Datei wird gerade geschrieben oder ist im Moment nicht lesbar; versuchen Sie es gleich noch einmal.
cli-file-too-large = die Datei ist größer, als Ibis liest, oder zu viele Dateien sind gleichzeitig geöffnet; teilen Sie die Bibliothek auf oder öffnen Sie weniger Dateien.
cli-file-missing = unter diesem Pfad gibt es keine Datei; prüfen Sie die Schreibweise oder geben Sie den vollständigen Pfad an.
cli-file-is-folder = das ist ein Ordner; geben Sie die Bibliotheksdatei darin an.
cli-file-permission = dieses Programm darf die Datei nicht lesen; prüfen Sie die Berechtigungen, oder führen Sie es außerhalb der Sandbox aus.
cli-zotero-unavailable = keine lokale Zotero-Bibliothek gefunden, oder sie war nicht lesbar; prüfen Sie, ob Zotero installiert ist und einmal geöffnet wurde, oder verwenden Sie --file.
cli-zotero-damaged = ihre Daten sind unverständlich; öffnen Sie Zotero einmal, damit es seine Datenbank reparieren kann.
cli-zotero-newer = eine neuere Zotero-Version hat sie geschrieben; aktualisieren Sie Ibis.
cli-zotero-too-large = sie ist größer, als Ibis auf einmal liest.
cli-key-empty = ein Zitierschlüssel darf nicht leer sein.
cli-search-partial = Keine Referenz passt zu allen Wörtern. Ähnlichste Treffer:
    { $rows }
cli-styles-showing = Angezeigt: { $shown } von { $total } Stilen; eingrenzen mit: ibis styles WORDS
cli-project-unreadable = { $name } konnte nicht gelesen werden: { $reason }.
cli-project-damaged = { $name } konnte nicht vollständig gelesen werden: prüfen Sie Zeile { $lines }.
cli-warning-text-kept = { $subject } konnte nicht vollständig umgewandelt werden; Ibis verwendet den Text wie geschrieben
cli-warning-date = { $subject } konnte nicht vollständig umgewandelt werden; Ibis hat die Referenz behalten; prüfen Sie das Datum vor dem Zitieren
cli-warning-link = { $subject } ist privat oder keine Webadresse; Ibis lässt es weg
cli-warning-names = { $subject } konnte nicht vollständig gelesen werden; Ibis verwendet die lesbaren Namen
cli-warning-fields-kept = { $subject } konnte nicht vollständig umgewandelt werden; Ibis hat die Referenz mit den lesbaren Feldern behalten
cli-warning-syntax-field = { $subject } konnte nicht gelesen werden; Ibis überspringt diesen Eintrag
cli-warning-duplicate-key-named = mehrere Einträge verwenden „{ $key }“ als Schlüssel oder ID, daher überspringt Ibis sie alle
cli-search-stale-at = Hinweis: { $name } konnte nicht aktualisiert werden; Ergebnisse vom Stand { $time }
cli-search-stale = Hinweis: { $name } konnte nicht aktualisiert werden; angezeigt wird der letzte erfolgreich gelesene Stand
cli-file-unopened = { $path } kann nicht geöffnet werden: { $reason }
cli-zotero-unreadable-profiles = die Zotero-Bibliothek kann nicht gelesen werden: { $reason } Profile: { $profiles }.
cli-zotero-unreadable = die Zotero-Bibliothek kann nicht gelesen werden: { $reason }
cli-key-duplicate = der Zitierschlüssel „{ $key }“ wird von mehreren Einträgen verwendet{ $at }, daher überspringt Ibis sie alle; geben Sie jedem Eintrag einen eigenen Schlüssel.
cli-key-damaged = der Eintrag „{ $key }“{ $at } konnte nicht gelesen werden, daher überspringt Ibis ihn{ $reason }; korrigieren Sie ihn in Ihrer Bibliothek.
cli-key-library-incomplete = „{ $key }“ nicht gefunden: { $reason }
cli-key-ambiguous = mehrere Referenzen verwenden den Zitierschlüssel „{ $key }“; geben Sie jeder in Ihrer Bibliothek einen eigenen Schlüssel.
cli-key-without-library = „{ $key }“ kann nicht nachgeschlagen werden: keine Bibliothek geöffnet; fügen Sie --file <library> oder --zotero hinzu.
cli-key-not-found-search = keine Referenz hat den Zitierschlüssel „{ $key }“; den richtigen Schlüssel finden Sie mit: ibis search <words>
cli-key-too-long = der Zitierschlüssel „{ $key }…“ ist zu lang.
cli-project-unavailable = die Datei ist nicht vorhanden oder noch nicht auf diesem Computer
cli-project-not-file = das ist keine Datei
cli-project-too-large = die Datei ist größer als 64 MiB
cli-project-not-utf8 = die Datei ist kein UTF-8-Text
cli-project-changing = die Datei ändert sich noch
cli-project-not-bibtex = ein Literaturverzeichnis muss eine BibTeX- oder BibLaTeX-Datei sein
cli-project-fields-differ = dieser Eintrag weicht von der Bibliothek ab
cli-project-case-differs = ein Schlüssel unterscheidet sich nur in der Groß-/Kleinschreibung
cli-project-alias = es ist der Alias eines anderen Eintrags
cli-project-helper = es gehört zu einem @xdata-Eintrag, der nicht zitiert werden kann
cli-project-commented = es steht in @comment, das manche Werkzeuge trotzdem lesen
cli-project-key-doubt = Schlüssel bezeichnet eventuell eine andere Referenz (Zeile { $line }): { $reason }
cli-destination-paste = Hinweis: Ibis fügt in { $name } ein.
cli-destination-unknown = Hinweis: Ibis kennt diese App nicht und fragt daher vor dem Einfügen.
cli-destination-unsupported = Hinweis: Ibis kopiert hier, weil es auf diesem System noch nicht einfügen kann.
cli-destination-terminal = Hinweis: Ibis kopiert für { $name }, weil Einfügen dort als Tastatureingabe ausgeführt wird.
cli-destination-remote = Hinweis: Ibis kopiert für { $name }, weil Tastenanschläge dort ein anderes System erreichen.
cli-destination-shortcut = Hinweis: Ibis kopiert für { $name }, weil dessen Einfügetaste etwas anderes bewirkt.
cli-destination-copy = Hinweis: Ibis kopiert für { $name }.
cli-doctor-all-good = Alles in Ordnung.
cli-doctor-one-finding = 1 Punkt zu prüfen.
cli-doctor-findings = { $count } Punkte zu prüfen.
cli-doctor-last-read = Zuletzt erfolgreich gelesen: { $age }.
cli-doctor-check-failed = Prüfung fehlgeschlagen.
cli-search-shorthand = Zitiert mit: { $cited }; gesucht nach: { $words }
cli-project-key-in = In { $name } (Zeile { $line })
cli-project-key-missing = Fehlt in { $name }
cli-project-key-unchecked = { $name } konnte nicht geprüft werden
cli-project-key-repeated = Schlüssel kommt mehrfach vor (Zeilen { $lines })
cli-citation-language = Hinweis: Sprache der Zitation: { $language }.
cli-library-stale-note = (letzter erfolgreich gelesener Stand; Aktualisierung fehlgeschlagen)
cli-library-watch-note = (Änderungen werden nicht automatisch übernommen)
cli-greeting = Ibis { $version } findet Referenzen und zitiert sie überall.

    Probieren Sie es aus:
      ibis search lantern --file refs.bib       Referenz finden
      ibis cite vale2024lanterns --file refs.bib --style apa
                                                Zitation erstellen
      ibis doctor --file refs.bib               Bibliothek prüfen

    Mit installierter Ibis-App können Sie --file weglassen.
    Mit --json eignet sich jeder Befehl für Skripte und Agenten. Alle Befehle: ibis --help

# Settings and ibis agents share these app connections.
settings-ai-apps = KI-Apps
ai-apps-group = Apps auf diesem Computer
ai-apps-command-line = Befehlszeile
ai-apps-description = Verbinden Sie Ibis mit den KI-Apps auf diesem Computer.
ai-apps-words = agenten ki MCP Claude Codex Cursor VS Code Gemini Windsurf LM Studio ChatGPT ki-apps assistenten
proposals-title = Vorgeschlagene Änderungen
proposals-description = Prüfen und übernehmen Sie Bibliotheksänderungen, die KI-Apps vorschlagen.
proposals-words = vorschläge änderungen referenz hinzufügen
proposals-checking = Vorgeschlagene Änderungen werden geprüft…
proposals-empty = Keine vorgeschlagenen Änderungen
proposals-from = { $app } · { $library }
proposals-apply = Übernehmen
proposals-reject = Ablehnen
proposals-app = { $app } · { $count } Vorschläge
proposals-reject-app = Alle ablehnen
proposals-applied = Übernommen; Sicherung als { $name } aufbewahrt.
proposals-failed = Diese Aktion konnte nicht abgeschlossen werden. Versuchen Sie es erneut.
proposals-read-failed = Vorgeschlagene Änderungen konnten nicht gelesen werden. Kehren Sie zu dieser Seite zurück, um es erneut zu versuchen.
app-failure-waiting-proposals = { proposals-read-failed }
app-failure-apply-proposal = { proposals-failed }
app-failure-reject-proposal = { proposals-failed }
app-failure-reject-app-proposals = { proposals-failed }
command-words = befehlszeile terminal shell konsole PATH agenten MCP
ai-apps-checking = KI-Apps werden geprüft…
ai-apps-not-connected = Nicht verbunden
ai-apps-connected = Verbunden. Starten Sie in der App eine neue Sitzung, um Ibis zu verwenden.
ai-apps-stale = Der ibis-Befehl wurde verschoben oder fehlt.
ai-apps-different = Diese App hat eine andere Ibis-Verbindung. Prüfen Sie die Änderung unten.
ai-apps-plugin = Über das Ibis-Plug-in verbunden.
ai-apps-connect = Verbinden
ai-apps-disconnect = Trennen
ai-apps-repair = Reparieren
ai-apps-replace = Ersetzen…
ai-apps-command-needed = Der ibis-Befehl fehlt. Installieren Sie ihn oben unter „Befehlszeile“, damit diese App Ibis starten kann.
ai-apps-failed = Die Verbindung zur KI-App konnte nicht geändert werden. Prüfen Sie die Einstellungen der App und die Ordnerberechtigungen, und versuchen Sie es erneut.
ai-apps-kept = Andere Server und Einstellungen bleiben erhalten. Neben jeder geänderten Datei wird eine Sicherung angelegt, wie bei ibis agents im Terminal.
app-failure-agent-apps = { ai-apps-failed }
app-failure-connect-agent-app = { ai-apps-failed }
app-failure-disconnect-agent-app = { ai-apps-failed }

# Native macOS picker and menu bar.
picker-native-app = Ibis
picker-native-panel = Ibis, Zitationsauswahl
picker-native-open = Ibis öffnen
picker-native-quit = Ibis beenden
picker-native-allow-paste = Einfügen erlauben…
picker-native-paste-copy-mode = Ibis kopiert Zitate. Drücken Sie ⌘V zum Einfügen.
picker-native-open-library = Bibliothek öffnen
picker-native-shortcut-unavailable = Das Tastenkürzel von Ibis ist nicht verfügbar.
picker-native-shortcut-not-saved = Ibis konnte nicht bestätigen, dass das Tastenkürzel gespeichert wurde.
picker-native-format = Format
picker-native-format-value = Format, { $format }
picker-native-citation = Zitat
picker-native-search-hint = Suchen Sie nach Titel, Autor, Jahr oder Schlüssel.
picker-native-choose-title = Bibliothek auswählen
picker-native-choose-hint = Wählen Sie eine BibTeX-, CSL-JSON-, RIS- oder CSV-Datei zum Durchsuchen.
picker-native-library-missing = { $name } wurde nicht gefunden
picker-native-library-scope = Bibliothek: { $name }
picker-native-missing-hint = Die Datei wurde möglicherweise verschoben oder umbenannt.
picker-native-find-library = { $name } suchen…
picker-native-zotero = Zotero
picker-native-untitled = Ohne Titel
picker-native-cited = Im Zitat
picker-native-journal-article = Zeitschriftenartikel
picker-native-other-reference = Referenz
picker-native-author-others = { $author } { row-et-al }
picker-native-unmatched-segment = Wählen Sie eine Referenz für „{ $query }“.
picker-native-token = Zitat: { $citation }
picker-native-active-token = Zitat bearbeiten: { $citation }
picker-native-citation-draft = Zitatentwurf
picker-native-locator-type = Art der Fundstelle
picker-native-edit-page = Seite oder Fundstelle bearbeiten
picker-native-return = Eingabetaste
picker-native-open-source = Quelle öffnen
picker-native-text-larger = Größere Schrift
picker-native-text-smaller = Kleinere Schrift
picker-native-density = Zeilen
picker-native-copied = Kopiert. Drücken Sie ⌘V zum Einfügen.
picker-native-access-title = { $app } einschalten
picker-native-access-drag = Falls es nicht in der Liste steht, ziehen Sie dieses Symbol hinein.
picker-native-access-wait = Dies schließt sich, sobald es eingeschaltet ist. Bis dahin werden Zitate zum Einfügen mit ⌘V kopiert.
picker-native-move-earlier = Nach vorne
picker-native-move-later = Nach hinten
picker-native-reference-count = { $count } Quellen
picker-native-read-locator = { $term } { $value }
picker-native-read-prefix = „{ $text }“ davor
picker-native-read-more =
    { $count ->
        [one] dann 1 weitere Quelle
       *[other] dann { $count } weitere Quellen
    }
picker-native-copy-failed = Die Zwischenablage hat sich geändert oder konnte nicht beschrieben werden.
picker-native-engine-disconnected = Die Ibis-Engine wurde beendet. Versuchen Sie es erneut.
picker-native-engine-invalid = Die Ibis-Engine hat eine ungültige Antwort gesendet.
picker-native-engine-peer = Ibis konnte seine lokale Engine nicht überprüfen.
picker-native-engine-newer = Eine neuere Ibis-Engine läuft. Bauen Sie die native App mit der neueren Version.
picker-native-engine-launch = Ibis konnte seine lokale Engine nicht starten.
picker-native-engine-busy = Ibis fügt gerade ein anderes Zitat ein.
picker-native-engine-changed = Das Zitat hat sich geändert. Prüfen Sie es vor dem nächsten Versuch.
picker-native-request-failed = Ibis konnte die Anfrage nicht abschließen. Versuchen Sie es erneut.
picker-native-before-example = siehe
picker-native-after-example = Hervorhebung hinzugefügt
picker-native-form-narrative = Name im Text
picker-native-copy-citation = Zitat kopieren
picker-native-paste-short = Einfügen
picker-native-no-matches-in = Keine Treffer in { $name }
picker-native-search-glyph = Symbol im Suchfeld
picker-native-search-glyph-bird = Ibis-Vogel
picker-native-search-glyph-magnifier = Lupe
picker-native-search-glyph-none = Keins
picker-native-search-glyph-words = Symbol Logo Vogel Lupe Suchleiste

picker-native-remember-format = Für { $app } merken
picker-native-format-project = Projekt: { $name }
picker-native-format-project-blocked = Öffne { $name } in der Desktop-Auswahl, um die Bibliografie vor dem Zitieren zu prüfen.
picker-native-use-usual = Wieder { $format } verwenden
picker-native-forget-format = Format für { $app } vergessen
