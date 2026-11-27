#!/usr/bin/env python3
"""Writes a made-up Confluence space export, as Confluence writes it as XML
or CSV, to try a large import with: a tree of pages ten wide with every
25th a folder, each page with the content most spaces hold (headings,
paragraphs, a list, a table, an info and a code macro, a link to another
page, now and then a table of contents), earlier versions of each page,
and attachments of random bytes, so that none is stored only once and the
ZIP is as large as they are.

    scripts/make-confluence-space.py xml space.zip --pages 5000 --versions 4 --attachments 600 --attachment-kb 250

scripts/measure-import.sh then imports it and tells how long that took and
how much memory Turtle held. The same arguments give the same space.
"""
import argparse
import csv
import io
import os
import random
import zipfile

PAGES = "com.atlassian.confluence.pages"
WORDS = "deploy server runbook release build check review plan team note guide setup access token search area folder page import export".split()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("form", choices=["xml", "csv"], help="the export's form")
    parser.add_argument("zip", help="where to write the ZIP")
    parser.add_argument("--pages", type=int, default=5000, help="pages and folders (default 5000)")
    parser.add_argument("--versions", type=int, default=4, help="earlier versions of each page (default 4)")
    parser.add_argument("--attachments", type=int, default=600, help="attachments, spread over the pages (default 600)")
    parser.add_argument("--attachment-kb", type=int, default=250, help="size of each attachment in KB (default 250)")
    args = parser.parse_args()
    Space(args).write()
    print(f"{args.zip}: {os.path.getsize(args.zip) // (1024 * 1024)} MB")


class Space:
    def __init__(self, args):
        self.args = args
        self.random = random.Random(7)
        ids = range(1, args.pages + 1)
        self.titles = {i: f"Page {i} {self.random.choice(WORDS)}" for i in ids}
        self.parent = {i: None if i == 1 else (i - 2) // 10 + 1 for i in ids}
        self.folders = {i for i in ids if i % 25 == 0}
        pages = [i for i in ids if i not in self.folders]
        self.attached = {}
        for a in range(args.attachments):
            name = f"picture-{a}.png" if a % 3 == 0 else f"file-{a}.bin"
            self.attached.setdefault(pages[a % len(pages)], []).append((900000 + a, name))
        self.blob = os.urandom(args.attachment_kb * 1024)

    def sentence(self, words=14):
        return " ".join(self.random.choice(WORDS) for _ in range(words)).capitalize() + "."

    def storage(self, i):
        """A page's content in Confluence's storage format."""
        other = self.titles[self.random.randint(1, len(self.titles))]
        parts = [f"<h1>{self.titles[i]}</h1>"]
        parts += [f"<p>{self.sentence()} <strong>{self.sentence(3)}</strong> {self.sentence()}</p>" for _ in range(6)]
        parts.append(f'<p>See <ac:link><ri:page ri:content-title="{other}" /></ac:link>.</p>')
        parts.append("<ul>" + "".join(f"<li><p>{self.sentence(6)}</p></li>" for _ in range(5)) + "</ul>")
        rows = "".join(f"<tr><td>{self.sentence(3)}</td><td>{self.sentence(3)}</td></tr>" for _ in range(6))
        parts.append(f"<table><tbody><tr><th>A</th><th>B</th></tr>{rows}</tbody></table>")
        parts.append(f'<ac:structured-macro ac:name="info"><ac:rich-text-body><p>{self.sentence()}</p></ac:rich-text-body></ac:structured-macro>')
        parts.append('<ac:structured-macro ac:name="code"><ac:parameter ac:name="language">bash</ac:parameter><ac:plain-text-body><![CDATA[make deploy\nmake check]]></ac:plain-text-body></ac:structured-macro>')
        if i % 7 == 0:
            parts.append('<ac:structured-macro ac:name="toc" />')
        for _, name in self.attached.get(i, []):
            if name.endswith(".png"):
                parts.append(f'<ac:image><ri:attachment ri:filename="{name}" /></ac:image>')
            else:
                parts.append(f'<p><ac:link><ri:attachment ri:filename="{name}" /></ac:link></p>')
        return "".join(parts)

    def write(self):
        with zipfile.ZipFile(self.args.zip, "w", zipfile.ZIP_DEFLATED, allowZip64=True) as zip:
            if self.args.form == "xml":
                self.write_xml(zip)
            else:
                self.write_csv(zip)
            for page, files in self.attached.items():
                for attachment_id, _ in files:
                    content = attachment_id.to_bytes(8, "big") + self.blob
                    zip.writestr(zipfile.ZipInfo(f"attachments/{1000 + page}/{attachment_id}/1"), content, zipfile.ZIP_STORED)

    def write_xml(self, zip):
        out = io.StringIO()
        out.write('<?xml version="1.0" encoding="UTF-8"?>\n<hibernate-generic datetime="2026-11-26 11:19:52.169">\n')
        out.write('<object class="Space" package="com.atlassian.confluence.spaces"><id name="id">1</id>'
                  '<property name="name"><![CDATA[Big Space]]></property><property name="key"><![CDATA[BIG]]></property></object>\n')

        def page(page_id, i, body_id, original):
            out.write(f'<object class="Page" package="{PAGES}"><id name="id">{page_id}</id><property name="title"><![CDATA[{self.titles[i]}]]></property>')
            if body_id:
                out.write(f'<collection name="bodyContents" class="java.util.Collection"><element class="BodyContent" package="com.atlassian.confluence.core"><id name="id">{body_id}</id></element></collection>')
            out.write('<property name="contentStatus"><![CDATA[current]]></property>')
            out.write(f'<property name="originalVersionId">{original}</property>' if original else '<property name="originalVersionId"/>')
            out.write(f'<property name="space" class="Space" package="com.atlassian.confluence.spaces"><id name="id">1</id></property><property name="position">{i}</property>')
            if self.parent[i]:
                out.write(f'<property name="parent" class="Page" package="{PAGES}"><id name="id">{1000 + self.parent[i]}</id></property>')
            out.write("</object>\n")

        def body(body_id, page_id, storage):
            # The export writes `]]>` inside CDATA as `]] >`.
            out.write(f'<object class="BodyContent" package="com.atlassian.confluence.core"><id name="id">{body_id}</id>'
                      f'<property name="body"><![CDATA[{storage.replace("]]>", "]] >")}]]></property>'
                      f'<property name="content" class="Page" package="{PAGES}"><id name="id">{page_id}</id></property><property name="bodyType">2</property></object>\n')

        for i in self.titles:
            page_id = 1000 + i
            if i in self.folders:
                page(page_id, i, None, None)
                continue
            page(page_id, i, 5000000 + i, None)
            body(5000000 + i, page_id, self.storage(i))
            for v in range(self.args.versions):
                earlier = 10000000 + i * 100 + v
                page(earlier, i, earlier + 50000000, page_id)
                body(earlier + 50000000, earlier, self.storage(i))
            for attachment_id, name in self.attached.get(i, []):
                out.write(f'<object class="Attachment" package="{PAGES}"><id name="id">{attachment_id}</id><property name="title"><![CDATA[{name}]]></property>'
                          f'<property name="version">1</property><property name="contentStatus"><![CDATA[current]]></property>'
                          f'<property name="containerContent" class="Page" package="{PAGES}"><id name="id">{page_id}</id></property></object>\n')
        out.write("</hibernate-generic>")
        zip.writestr("entities.xml", out.getvalue())
        zip.writestr("exportDescriptor.properties", "exportType=space\n")

    def write_csv(self, zip):
        def table(name, columns, rows):
            out = io.StringIO()
            writer = csv.writer(out)
            writer.writerow(columns)
            writer.writerows(rows)
            # Named .gz, but not gzipped, as Confluence Cloud often writes them.
            zip.writestr(f"{name}.csv.gz", out.getvalue())

        content, bodies = [], []
        for i in self.titles:
            page_id = 1000 + i
            parent = 1000 + self.parent[i] if self.parent[i] else ""
            if i in self.folders:
                content.append([page_id, "FOLDER", self.titles[i], 1, "", "current", "", 1, i, parent])
                continue
            content.append([page_id, "PAGE", self.titles[i], self.args.versions + 1, "", "current", "", 1, i, parent])
            bodies.append([5000000 + i, self.storage(i), page_id, 2])
            for v in range(self.args.versions):
                earlier = 10000000 + i * 100 + v
                content.append([earlier, "PAGE", self.titles[i], v + 1, page_id, "current", "", 1, i, parent])
                bodies.append([earlier + 50000000, self.storage(i), earlier, 2])
            for attachment_id, name in self.attached.get(i, []):
                content.append([attachment_id, "ATTACHMENT", name, 1, "", "current", page_id, 1, "", ""])
        table("content", ["contentid", "contenttype", "title", "version", "prevver", "content_status", "pageid", "spaceid", "child_position", "parentid"], content)
        table("bodycontent", ["bodycontentid", "body", "contentid", "bodytypeid"], bodies)
        table("spaces", ["spaceid", "spacename", "spacekey"], [[1, "Big Space", "BIG"]])
        zip.writestr("exportDescriptor.properties.gz", "exportFormat=csv\nexportType=space\n")


if __name__ == "__main__":
    main()
