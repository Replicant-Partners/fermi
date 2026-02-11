# Fermi Notebook Frontend

> Observable-style reactive notebook for Fermi Probabilistic Language (FPL)

## Architecture

This is a **strictly typed, modular Svelte application** for creating and executing FPL forecasting notebooks.

### Design Principles

1. **Strict Type Safety**: Every data structure is strongly typed via TypeScript
2. **Modular Components**: Each cell type is self-contained with clear boundaries
3. **Reactive Execution**: Cells automatically re-execute when dependencies change
4. **Clean Separation**: Frontend ↔ Backend communication through typed API client
5. **Complexity Management**: Dependency graph logic isolated in stores

### Directory Structure

```
src/
├── lib/
│   ├── types/
│   │   ├── notebook.ts    # Core type definitions (Cell, Notebook, etc.)
│   │   └── api.ts         # API request/response types
│   ├── api/
│   │   └── client.ts      # Type-safe HTTP client
│   ├── stores/
│   │   └── notebook.ts    # Reactive state management
│   └── components/
│       ├── Notebook.svelte       # Main orchestrator
│       └── cells/
│           ├── QuestionCell.svelte
│           ├── DriverCell.svelte
│           ├── ModelCell.svelte (TODO)
│           └── MarkdownCell.svelte (TODO)
└── App.svelte
```

### Type System

**Cell Types** (Union Type):
- `QuestionCell` - Forecasting question with base rate
- `DriverCell` - Probabilistic variable (continuous/discrete/binary)
- `EvidenceCell` - Updates to driver beliefs
- `ModelCell` - FPL expression combining drivers
- `SimulationCell` - Monte Carlo execution config
- `MarkdownCell` - Rich text documentation
- `VisualizationCell` - Charts and plots

**Visibility Levels**:
- `private` - Only owner can access
- `shared` - Team/organization access
- `public` - Publicly accessible

### Reactive Execution Flow

```
1. User edits Cell A
   ↓
2. Store updates cell
   ↓
3. Dependency graph identifies dependents (Cells B, C)
   ↓
4. Topological sort determines execution order
   ↓
5. Execute cells in order: A → B → C
   ↓
6. UI reactively updates
```

### API Integration

All backend communication goes through `lib/api/client.ts`:

```typescript
// Example: Execute a notebook
import { executeNotebook } from '$lib/api/client';

const response = await executeNotebook('notebook-123', {
  iterations: 10000
});
// response.cells contains outputs for all cells
```

### Backend Requirements

The backend must implement these endpoints:

```
GET    /api/notebooks/:id
POST   /api/notebooks
PUT    /api/notebooks/:id
DELETE /api/notebooks/:id
POST   /api/notebooks/:id/execute
POST   /api/notebooks/:id/cells/:cellId/execute
POST   /api/notebooks/:id/assist  (agent integration)
```

See `src/lib/types/api.ts` for full request/response schemas.

## Development

```bash
# Install dependencies
npm install

# Run dev server
npm run dev

# Build for production
npm run build

# Preview production build
npm run preview
```

## Deployment

### Build for fermi.systems

```bash
# Set production API URL
echo "VITE_API_BASE_URL=https://agent-bestiary.world/api" > .env

# Build
npm run build

# Output goes to dist/
# Copy to fermi backend: ~/fermi/static/fermi-app/
```

### Integration with Fermi Backend

```bash
# From fermi-notebook/
npm run build

# Copy to backend static directory
cp -r dist/* ~/fermi/static/fermi-app/
```

## Agent Integration (Fermi Coach)

The notebook supports agent-assisted authoring via the `/assist` endpoint:

```typescript
import { getAgentAssist } from '$lib/api/client';

const response = await getAgentAssist('notebook-123', {
  intent: "Add a driver for AI chip demand growth",
  context: {
    current_cells: notebook.cells,
    active_cell_id: 'cell-5'
  },
  agent_id: 'fermi_coach'  // or 'monte_carlo_sim'
});

// response.suggestions contains AgentSuggestion[]
// Each suggestion can insert/update cells
```

## Brier Scoring & Calibration

Notebooks support forecast resolution and calibration tracking:

```typescript
import { resolveForecast, getCalibration } from '$lib/api/client';

// Resolve a forecast
await resolveForecast({
  notebook_id: 'nb-123',
  question_cell_id: 'cell-1',
  actual_outcome: true,
  resolution_source: 'Official announcement'
});

// Get calibration data
const calibration = await getCalibration({
  user_id: 'user-456',
  time_range: { start: '2026-01-01', end: '2026-12-31' }
});
// calibration.overall_brier, calibration.bins
```

## Design System (Gruvbox + Tufte)

The UI uses:
- **Gruvbox color palette** (matching Agent Bestiary World)
- **Tufte principles**: High data-ink ratio, minimal chrome
- **Responsive typography**: Font weights 300-500, generous line height
- **Subtle interactions**: Light borders, breathing room

### Colors

```css
--bg0-hard: #1d2021;   /* Background */
--fg0: #fbf1c7;        /* Primary text */
--aqua: #8ec07c;       /* Success/results */
--orange: #fe8019;     /* Accent */
--red: #fb4934;        /* Errors */
```

## Complexity Management

### Modular Cell Components

Each cell type is **fully self-contained**:
- Props: `cell: CellType`, `readonly: boolean`
- Emits: Updates via store actions, not events
- No parent dependencies (except stores)

### Store Pattern

```typescript
// ✅ Good: Use stores for cross-component state
import { updateCell } from '$lib/stores/notebook';
updateCell('cell-123', { output: newValue });

// ❌ Bad: Direct mutation
cell.output = newValue;
```

### Type Guards

```typescript
// Use type narrowing for cell-specific logic
if (cell.type === 'driver') {
  // TypeScript knows cell is DriverCell here
  console.log(cell.distribution);
}
```

## Future Features

- [ ] Real-time collaboration (WebSocket)
- [ ] Cell comments & discussions
- [ ] Notebook forking
- [ ] Public gallery
- [ ] D3.js visualizations (histograms, tornado charts)
- [ ] Monaco editor for FPL syntax
- [ ] Keyboard shortcuts (Jupyter-style)
- [ ] Export to PDF/Markdown

## License

MIT
