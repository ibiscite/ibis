// Excerpted 2026-11-14 from Juris-M/zotero-odf-scan-plugin v3.2.2
// Revision 27a6ae72e6bb048fa808cd987f95327f67efd70f. See README.md and ../../LICENSE.
/*
    ***** BEGIN LICENSE BLOCK *****

    Copyright © 2009 Center for History and New Media
                     George Mason University, Fairfax, Virginia, USA
                     http://zotero.org

    This file is part of Zotero.

    Zotero is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.
    Zotero is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with Zotero.  If not, see <http://www.gnu.org/licenses/>.

    ***** END LICENSE BLOCK *****
*/
var ODFConv = function () {};
// odfConverter.js lines 310-310
        let tmplCitation = "<text:reference-mark-start text:name=\"ZOTERO_ITEM {&quot;properties&quot;:{&quot;formattedCitation&quot;:&quot;%{1}s&quot;},&quot;citationItems&quot;:%{2}s} RND%{3}s\"/>%{4}s<text:reference-mark-end text:name=\"ZOTERO_ITEM {&quot;properties&quot;:{&quot;formattedCitation&quot;:&quot;%{5}s&quot;},&quot;citationItems&quot;:%{6}s} RND%{7}s\"/>";
// odfConverter.js lines 330-335
        let rexFixMarkupBold = /[\*][\*](.*?)[\*][\*]/;
        let rexFixMarkupItalic = /\*(.*?)\*/;

        let rexTextAll = /{\s*([^|{}]*)\|\s*([^|}]*)\s*\|\s*([^|}]*)\s*\|([^|}]*?)\s*\|\s*([^|}]*)\s*}/g;
        let rexText = /{\s*([^|{}]*)\|\s*([^|}]*)\s*\|\s*([^|}]*)\s*\|([^|}]*?)\s*\|\s*([^|}]*)\s*}/;
        let rexTextPlain = /{[^|{}]*\|[^|}]*\|[^|}]*\|[^|}]*\|[^|}]*}/;
// odfConverter.js lines 613-746
        ODFConv.prototype.composeCitations = function () {
            // Split file string to twin lists
            // and reverse iterate over cites (master recomposition loop)
            // compose items
            // compose citation
            // recompose document
            let ret = [];
            let items = [];
            // Some jiggery-pokery is needed to get a nested
            // list out of JavaScript regexp (as from Python re.findall)
            let m = [];
            let m_all = this.content.match(rexTextAll);
            for (let i=0,ilen=m_all.length;i<ilen;i+=1) {
                let subm = [];
                let m_one = m_all[i].match(rexText);
                for (let j=1,jlen=m_one.length;j<jlen;j+=1) {
                    subm.push(m_one[j]);
                }
                m.push(subm);
            }
            let lst = this.content.split(rexTextPlain);
            ret.push(lst.slice(-1)[0]);
            let placeholder = [];
            for (let i=m.length-1;i>-1;i+=-1) {
                let item = {};
                let plaintextcite = m[i][1].replace(/^\s+/,"").replace(/\s+$/,"").split("\"").join("");
                if (plaintextcite && plaintextcite[0] === "-") {
                    item["suppress-author"] = true;
                    plaintextcite = plaintextcite.slice(1);
                }
                placeholder.push(plaintextcite);
                let link = this.fixMarkup(m[i][4]).replace(/^\s+/,"").replace(/\s+$/,"");
                item.prefix = this.fixMarkup(m[i][0]).replace(/^\s+/,"");
                item.locator = this.fixMarkup(m[i][2]).replace(/^\s+/,"").replace(/\s+$/,"");
                item.suffix = this.fixMarkup(m[i][3]).replace(/\s+$/,"");
                // extract the key
                let params = {};
                if (link.slice(0,22) === "zotero://select/items/") {
                    params.offset = 22;
                    params.splitter = "_";
                    params.fromZoteroSelect = true;
                } else {
                    // Assuming two-char prefix, like zu: or zg:
                    // By flagging the app as well as the library type,
                    // we leave the door open on support for eclectic
                    // citation sources (mixing embedded metadata from
                    // Zotero, Papers, Mendeley, etc). Not practical
                    // yet, but one day ...
                    params.offset = 3;
                    params.splitter = ":";
                    params.fromZoteroSelect = false;
                    if (link.slice(1,2) === "u") {
                        params.isUserItem = true;
                    }
                }
                let myid = link.slice(params.offset);
                let myidlst = myid.split(params.splitter);
                if (myidlst.length === 2) {
                    // the real deal. construct uris
                    item.key = myidlst[1];
                    if (params.isUserItem) {
                        // If we ever want to go back to using the local UserID, this should work
                        // let userID = Zotero.Users.getCurrentUserID();
                        if (myidlst[0] == "0") {
                            userID = 'local/' + Zotero.Users.getLocalUserKey();
                        }
                        else {
                            userID = myidlst[0];
                        }
                        item.uri = ["http://zotero.org/users/" + userID + "/items/" + myidlst[1]];
                        item.uris = item.uri.slice();
                    } else {
                        let groupID = myidlst[0];
                        if (params.fromZoteroSelect) {
                            groupID = Zotero.Groups.getGroupIDFromLibraryID(myidlst[0]);
                        }
                        item.uri = ["http://zotero.org/groups/" + groupID + "/items/" + myidlst[1]];
                        item.uris = item.uri.slice();
                    }
                } else {
                    // punt
                    item.key = myidlst[0];
                }
                items = [item].concat(items);
                if (lst[i]) {
                    placeholder = placeholder.join("; ")
                        .split("\"").join("")
                        .split(/[\\]*&quot;/).join("");
                    let escapedPlaceholder = placeholder.split("&").join("&amp;");
                    items = JSON.stringify(items);
                    items = items.split("\\\\&quot;").join("\\&quot;");
                    items = items.split("\"").join("&quot;");
                    let randstr = this.generateRandomString();
                    let citation = tmplCitation.replace("%{1}s", escapedPlaceholder)
                        .replace("%{2}s",items)
                        .replace("%{3}s",randstr)
                        .replace("%{4}s",placeholder)
                        .replace("%{5}s",escapedPlaceholder)
                        .replace("%{6}s",items)
                        .replace("%{7}s",randstr);
                    //Zotero.debug(citation)
                    ret.push(citation);
                    ret.push(lst[i]);
                    items = [];
                    placeholder = [];
                }
            }
            ret.reverse();
            this.content = ret.join("");

        };

        ODFConv.prototype.fixMarkup = function (str) {
            str = str.replace(rexFixMarkupBold, "&lt;b&gt;$1&lt;/b&gt;");
            str = str.replace(rexFixMarkupItalic, "&lt;i&gt;$1&lt;/i&gt;");
            str = str.split("&quot;").join("\\&quot;");
            str = str.split("\"").join("\\&quot;");
            return str;
        };
        ODFConv.prototype.generateRandomString = function (str) {
            let randstr;
            while (true) {
                let nums = [49,50,51,52,53,54,55,56,57,65,66,67,68,69,70,71,72,73,74,75,76,77,78,79,80,81,82,83,84,85,86,87,88,89,90,97,98,99,100,101,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,121,122];
                randstr = "";
                for (let i=0,ilen=10;i<ilen;i+=1) {
                    randstr += String.fromCharCode(nums[parseInt(Math.random() * 61)]);
                }
                if (!this.rands[randstr]) {
                    this.rands[randstr] = true;
                    break;
                }
            }
            return randstr;
        };
