<script>
  import { getConfigTree } from '../api.js';
  import { Box } from '@plures/design-dojo/layout';
  import { Button, Text } from '@plures/design-dojo/primitives';
  import { List, ListItem } from '@plures/design-dojo/data';
  import { Pane } from '@plures/design-dojo/surfaces';
  import { EmptyState } from '@plures/design-dojo';

  let tree = $state(null);
  let loading = $state(true);
  let selected = $state(null);

  $effect(() => { loadConfig(); });

  async function loadConfig() {
    loading = true;
    try {
      const result = await getConfigTree();
      tree = result;
    } catch {
      tree = {
        name: 'datacenter',
        children: [
          { name: 'cluster-01', children: [
            { name: 'node-a.yaml', content: 'role: compute\ncpu: 64\nmemory: 256GB' },
            { name: 'node-b.yaml', content: 'role: storage\ncpu: 32\nmemory: 128GB' },
          ]},
          { name: 'cluster-02', children: [
            { name: 'node-c.yaml', content: 'role: compute\ncpu: 128\nmemory: 512GB' },
          ]},
          { name: 'global.yaml', content: 'region: westus2\nenv: production' },
        ],
      };
    }
    loading = false;
  }

  function selectNode(node) {
    selected = node;
  }

  /** Flatten tree into items for List rendering */
  function flattenTree(node, depth = 0) {
    if (!node) return [];
    const items = [{ node, depth }];
    if (node.children) {
      for (const child of node.children) {
        items.push(...flattenTree(child, depth + 1));
      }
    }
    return items;
  }

  let flatItems = $derived(tree ? flattenTree(tree) : []);
</script>

<Box padding={4} class="config-browser">
  <Box border="none" class="cb-header">
    <Text>Config Browser</Text>
    <Button variant="outline" size="sm" onclick={loadConfig}>↻ Reload</Button>
  </Box>

  <Box border="none" class="cb-content">
    <Box border="none" class="tree-panel">
      {#if loading}
        <Text>Loading...</Text>
      {:else}
        <List>
          {#each flatItems as item (item.node.name + item.depth)}
            <ListItem onclick={() => selectNode(item.node)}>
              {#snippet children()}
                <Text class="tree-label" style="padding-left: {item.depth * 16}px">
                  {item.node.children ? '📁' : '📄'} {item.node.name}
                </Text>
              {/snippet}
            </ListItem>
          {/each}
        </List>
      {/if}
    </Box>

    {#if selected?.content}
      <Pane class="detail-panel">
        <Text>{selected.name}</Text>
        <Text monospace class="config-content">{selected.content}</Text>
      </Pane>
    {/if}
  </Box>
</Box>

<style>
  :global(.config-browser) {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  :global(.cb-header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }

  :global(.cb-content) {
    display: flex;
    flex: 1;
    gap: 12px;
    overflow: hidden;
  }

  :global(.tree-panel) {
    flex: 1;
    overflow-y: auto;
  }

  :global(.detail-panel) {
    flex: 1;
    overflow-y: auto;
  }

  :global(.tree-label) {
    font-size: 12px;
    cursor: pointer;
  }

  :global(.config-content) {
    font-size: 12px;
    white-space: pre-wrap;
  }
</style>
