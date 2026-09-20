import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { createReadStream, readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

// Both development and packaged previews use the same locally bundled PDF resources.
const pdfRoot = fileURLToPath(new URL('./node_modules/pdfjs-dist/', import.meta.url));
const pdfAssets = ['cmaps', 'standard_fonts', 'wasm'].flatMap((directory) =>
  readdirSync(`${pdfRoot}${directory}`, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => `${directory}/${entry.name}`),
);
export default defineConfig({
  plugins: [
    react(),
    {
      name: 'pdf-resources',
      generateBundle() {
        for (const name of pdfAssets)
          this.emitFile({ type: 'asset', fileName: `pdf-assets/${name}`, source: readFileSync(`${pdfRoot}${name}`) });
      },
      configureServer(server) {
        server.middlewares.use('/pdf-assets/', (request, response, next) => {
          const name = request.url?.slice(1);
          if (!name || !pdfAssets.includes(name)) return next();
          response.setHeader('Content-Type', name.endsWith('.wasm') ? 'application/wasm' : 'application/octet-stream');
          createReadStream(`${pdfRoot}${name}`).pipe(response);
        });
      },
    },
  ],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: 'safari15', chunkSizeWarningLimit: 2000 },
});
