<script>
  import { List, ListItem } from '@plures/design-dojo/data';
  import { Button, Toggle, Text } from '@plures/design-dojo/primitives';
  import { Box } from '@plures/design-dojo/layout';
  import { pluginRegistry, togglePlugin } from './plugins/registry.js';
</script>

<Box padding={4} class="plugin-manager">
  <Text class="plugin-title">Extensions</Text>
  <List>
    {#each $pluginRegistry as plugin (plugin.id)}
      <ListItem>
        {#snippet children()}
          <Box class="plugin-row">
            <Text class="plugin-icon">{plugin.icon}</Text>
            <Box class="plugin-info">
              <Text>{plugin.name}</Text>
              <Text class="plugin-desc">{plugin.description}</Text>
            </Box>
            <Button
              variant={plugin.enabled ? 'outline' : 'ghost'}
              size="sm"
              onclick={() => togglePlugin(plugin.id)}
            >
              {plugin.enabled ? 'Disable' : 'Enable'}
            </Button>
          </Box>
        {/snippet}
      </ListItem>
    {/each}
  </List>
</Box>

<style>
  :global(.plugin-manager) {
    overflow-y: auto;
    height: 100%;
  }

  :global(.plugin-title) {
    font-size: 16px;
    font-weight: 600;
    margin-bottom: 16px;
  }

  :global(.plugin-row) {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
  }

  :global(.plugin-icon) {
    font-size: 24px;
    flex-shrink: 0;
  }

  :global(.plugin-info) {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  :global(.plugin-desc) {
    font-size: 12px;
    color: var(--text-secondary, #8b90a0);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
