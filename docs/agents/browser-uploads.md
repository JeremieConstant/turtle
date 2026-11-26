# Trying a file upload in the browser pane

The browser pane cannot drive the operating system's file picker, so a button that opens it (Import Markdown, Import from Confluence, attaching a file) needs the file handed to the page instead. People choose the file in the picker as usual; this is only for trying it by hand.

1. Start a Turtle of your own (`scripts/try.sh`, or a `.claude/launch.json` entry running it) and sign in from the page by sending `seed-account.json` to `sign_in`, as `AGENTS.md` says.
2. Serve the file to the page from a directory outside the repository, such as your scratchpad. The page needs CORS to read it:

   ```bash
   python3 -I -c 'import http.server,functools,sys
   class H(http.server.SimpleHTTPRequestHandler):
       def end_headers(self): self.send_header("Access-Control-Allow-Origin","*"); super().end_headers()
   http.server.ThreadingHTTPServer(("127.0.0.1",4198),functools.partial(H,directory=sys.argv[1])).serve_forever()' <directory>
   ```

   Run it in the background and stop it when done.
3. In the page, fetch the file and answer the next file picker with it:

   ```js
   const blob = await fetch('http://127.0.0.1:4198/<file>').then(r => r.blob())
   const file = new File([blob], '<file>', { type: blob.type })
   HTMLInputElement.prototype.click = function () {
     if (this.type !== 'file') return
     const transfer = new DataTransfer()
     transfer.items.add(file)
     this.files = transfer.files
     this.dispatchEvent(new Event('change'))
   }
   ```

   The override lives until the page reloads: navigate inside the app (clicking links), not with a fresh page load, between this step and the next.
4. Click the button that opens the picker. The upload runs as it does for a person, toasts included.

A real export from someone's Confluence holds their data: keep it, and every copy you make of it, outside the repository, and delete the copies when done.
