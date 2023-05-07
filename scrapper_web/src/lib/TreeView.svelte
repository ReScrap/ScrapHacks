
<script>
  export let tree;
  export let scrap;
  export let label=undefined;
  let expanded = false;
  function toggleExpansion() {
    expanded = !expanded;
  };
  function download() {
    console.log({label,tree});
    scrap.postMessage({download:{label,...tree}});
    console.log(tree);
  }
</script>

<ul>
  <li>
    {#if tree.type == "directory" && tree.entries}
      <span on:click={toggleExpansion} on:keydown={toggleExpansion}>
        {#if expanded}
            <span class="arrow">[-]</span>
        {:else}
            <span class="arrow">[+]</span>
        {/if}
        {label}
      </span>
      {#if tree.entries && expanded}
        {#each [...tree.entries] as [name, child]}
          <svelte:self {scrap} label={name} tree={child} />
        {/each}
      {/if}
    {:else}
      <span>
        <span class="no-arrow" />
        <a href="#download" title="{tree.size} bytes" on:click={download}>{label}</a>
      </span>
    {/if}
  </li>
</ul>

<style>
  ul {
    margin: 0;
    list-style: none;
    padding-left: 1.2rem;
    user-select: none;
  }
  .no-arrow {
    padding-left: 1rem;
  }
  .arrow {
    cursor: pointer;
    display: inline-block;
  }
</style>
