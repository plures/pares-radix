<script>
  import { getMemories, getPraxisGuidance, getAnalysisEvents, triggerPraxisAnalysis, getSourceSpans } from './api.js';
  import { Box, Tabs } from '@plures/design-dojo/layout';
  import { Button, Text } from '@plures/design-dojo/primitives';
  import { List, ListItem } from '@plures/design-dojo/data';
  import { Pane } from '@plures/design-dojo/surfaces';

  const MAX_CONTENT_PREVIEW_LENGTH = 120;

  const GUIDANCE_CATEGORIES = [
    { id: 'facts', name: 'Facts', icon: '📊' },
    { id: 'rules', name: 'Rules', icon: '📋' },
    { id: 'constraints', name: 'Constraints', icon: '⚠️' },
    { id: 'decisions', name: 'Decisions', icon: '✅' },
    { id: 'risks', name: 'Risks', icon: '⚠️' },
    { id: 'guidance', name: 'Guidance', icon: '💡' },
  ];

  /** @type {{ id: string, content: string, category: string, created_at: string }[]} */
  let memories = $state([]);
  
  /** @type {Record<string, Array<{ id: string, content: string, confidence: number, priority: number, source_spans: string[] }>>} */
  let guidanceData = $state({});
  
  /** @type {Array<{ id: string, event_type: string, timestamp: string, guidance_updated: number }>} */
  let analysisEvents = $state([]);
  
  let selectedGuidanceCategory = $state('facts');
  let isAnalyzing = $state(false);

  async function refreshMemories() {
    try { memories = await getMemories(); } catch { /* non-critical */ }
  }

  async function refreshGuidance() {
    try {
      for (const category of GUIDANCE_CATEGORIES) {
        const guidance = await getPraxisGuidance(category.id);
        guidanceData[category.id] = guidance;
      }
      guidanceData = { ...guidanceData };
    } catch (error) {
      console.warn('Failed to load Praxis guidance:', error);
    }
  }

  async function refreshAnalysisEvents() {
    try { analysisEvents = await getAnalysisEvents(5); } catch { /* non-critical */ }
  }

  async function triggerAnalysis() {
    if (isAnalyzing) return;
    isAnalyzing = true;
    try {
      await triggerPraxisAnalysis();
      await refreshGuidance();
      await refreshAnalysisEvents();
    } catch (error) {
      console.error('Failed to trigger analysis:', error);
    } finally {
      isAnalyzing = false;
    }
  }

  async function showSourceSpansHandler(spanIds) {
    try {
      const spans = await getSourceSpans(spanIds);
      console.log('Source spans:', spans);
    } catch (error) {
      console.error('Failed to load source spans:', error);
    }
  }

  $effect(() => {
    refreshMemories();
    refreshGuidance();
    refreshAnalysisEvents();
    const memoryInterval = setInterval(refreshMemories, 5000);
    const guidanceInterval = setInterval(refreshGuidance, 10000);
    const eventsInterval = setInterval(refreshAnalysisEvents, 8000);
    return () => {
      clearInterval(memoryInterval);
      clearInterval(guidanceInterval);
      clearInterval(eventsInterval);
    };
  });

  const guidanceTabs = GUIDANCE_CATEGORIES.map(c => ({ key: c.id, label: c.name, icon: c.icon }));
</script>

<Box border="none" class="memory-sidebar">
  <!-- Memory Section -->
  <Pane class="sidebar-section">
    <Box border="none" class="section-header">
      <Text>🧠 Memories</Text>
    </Box>
    <List class="memory-list">
      {#if memories.length === 0}
        <ListItem>
          {#snippet children()}
            <Text class="text-muted">No memories yet.</Text>
          {/snippet}
        </ListItem>
      {:else}
        {#each memories as m (m.id)}
          <ListItem>
            {#snippet children()}
              <Box border="none" class="memory-item">
                <Box border="none" class="memory-meta-row">
                  <Text class="memory-category">{m.category}</Text>
                  {#if m.created_at}
                    <Text class="memory-time">
                      {new Date(m.created_at).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })}
                    </Text>
                  {/if}
                </Box>
                <Text class="memory-content">
                  {m.content.length > MAX_CONTENT_PREVIEW_LENGTH ? m.content.slice(0, MAX_CONTENT_PREVIEW_LENGTH) + '…' : m.content}
                </Text>
              </Box>
            {/snippet}
          </ListItem>
        {/each}
      {/if}
    </List>
  </Pane>

  <!-- Praxis Guidance Section -->
  <Pane class="sidebar-section">
    <Box border="none" class="section-header">
      <Text>🎯 Praxis Guidance</Text>
      <Button variant="ghost" size="sm" onclick={triggerAnalysis} disabled={isAnalyzing}>
        {isAnalyzing ? '⏳' : '🔄'}
      </Button>
    </Box>

    <Tabs tabs={guidanceTabs} activeTab={selectedGuidanceCategory} ontabchange={(key) => selectedGuidanceCategory = key}>
      {#snippet children({ activeTab: currentTab })}
        <Box border="none" class="guidance-content">
          {#if guidanceData[currentTab]?.length > 0}
            <List>
              {#each guidanceData[currentTab] as guidance (guidance.id)}
                <ListItem>
                  {#snippet children()}
                    <Box border="none" class="guidance-item">
                      <Box border="none" class="guidance-header-row">
                        <Text class="confidence">{(guidance.confidence * 100).toFixed(0)}%</Text>
                        <Text class="priority">P{guidance.priority}</Text>
                      </Box>
                      <Text class="guidance-text">{guidance.content}</Text>
                      {#if guidance.source_spans?.length > 0}
                        <Button variant="ghost" size="sm" onclick={() => showSourceSpansHandler(guidance.source_spans)}>
                          📎 {guidance.source_spans.length} source{guidance.source_spans.length !== 1 ? 's' : ''}
                        </Button>
                      {/if}
                    </Box>
                  {/snippet}
                </ListItem>
              {/each}
            </List>
          {:else}
            <Box border="none" class="guidance-empty">
              <Text>💭</Text>
              <Text class="text-muted">No {GUIDANCE_CATEGORIES.find(c => c.id === currentTab)?.name.toLowerCase()} yet</Text>
              <Text class="text-hint">Chat more to generate guidance</Text>
            </Box>
          {/if}
        </Box>
      {/snippet}
    </Tabs>
  </Pane>

  <!-- Analysis Activity -->
  {#if analysisEvents.length > 0}
    <Pane class="sidebar-section">
      <Box border="none" class="section-header">
        <Text>⚡ Analysis Activity</Text>
      </Box>
      <List>
        {#each analysisEvents as event (event.id)}
          <ListItem>
            {#snippet children()}
              <Box border="none" class="activity-row">
                <Text class="activity-time">
                  {new Date(event.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                </Text>
                <Text>{event.guidance_updated} guidance updated</Text>
              </Box>
            {/snippet}
          </ListItem>
        {/each}
      </List>
    </Pane>
  {/if}
</Box>

<style>
  :global(.memory-sidebar) {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1rem;
    height: 100%;
    overflow-y: auto;
  }

  :global(.sidebar-section) {
    overflow: hidden;
  }

  :global(.section-header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.5rem 0;
    font-weight: 600;
  }

  :global(.memory-list) {
    max-height: 300px;
    overflow-y: auto;
  }

  :global(.memory-item) {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  :global(.memory-meta-row) {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  :global(.memory-category) {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--accent, #7c6af7);
  }

  :global(.memory-time) {
    font-size: 10px;
    color: var(--text-muted, #555);
  }

  :global(.memory-content) {
    font-size: 12px;
    color: var(--text-secondary, #8b90a0);
  }

  :global(.text-muted) { color: var(--text-muted, #555a6a); }
  :global(.text-hint) { font-size: 12px; color: var(--text-muted, #555a6a); }

  :global(.guidance-content) {
    max-height: 250px;
    overflow-y: auto;
  }

  :global(.guidance-item) {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  :global(.guidance-header-row) {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  :global(.confidence) {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent, #7c6af7);
  }

  :global(.priority) {
    font-size: 10px;
    padding: 1px 4px;
    border-radius: 3px;
    background: var(--bg-elevated);
  }

  :global(.guidance-text) {
    font-size: 12px;
    color: var(--text-secondary, #8b90a0);
  }

  :global(.guidance-empty) {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 24px;
    gap: 4px;
  }

  :global(.activity-row) {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  :global(.activity-time) {
    font-size: 11px;
    color: var(--text-muted, #555);
    font-family: var(--font-mono, monospace);
  }
</style>
