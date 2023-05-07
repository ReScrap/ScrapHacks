import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import wasmPack from 'vite-plugin-wasm-pack';
import preprocess from 'svelte-preprocess';

export default defineConfig({
  plugins: [wasmPack("./scrapper/"),svelte({
    preprocess: preprocess({ postcss: true }) 
  })]
});
