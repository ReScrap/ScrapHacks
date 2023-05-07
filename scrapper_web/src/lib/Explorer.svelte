<script>
  import { onMount } from "svelte";
  import TreeView from "./TreeView.svelte";
  import ScrapWorker from "../scrapper.worker?worker";
  let worker;
  let tree;
  let busy;
  busy = false;
  onMount(async () => {
    worker = new ScrapWorker();
    worker.onmessage = (msg) => {
      console.log({ msg });
      if (msg.data) {
        if (msg.data.parse) {
          tree = msg.data.parse;
          busy = false;
        }
        if (msg.data.download) {
          let [file_name, url] = msg.data.download;
          let dl = document.createElement("a");
          dl.href = url;
          dl.download = file_name;
          dl.click();
        }
      }
    };
  });
  let files;
  function process() {
    console.log({ files });
    busy = true;
    worker.postMessage({ parse: files });
  }
</script>

<div class:lds-dual-ring={busy}>
  <input
    type="file"
    multiple
    accept=".packed"
    class="file-input file-input-bordered w-full max-w-xs"
    disabled={busy}
    bind:files
    on:change={process}
  />
</div>

{#if tree}
  {#each [...tree.entries] as [name, child]}
    <TreeView scrap={worker} label={name} tree={child} />
  {/each}
{/if}
