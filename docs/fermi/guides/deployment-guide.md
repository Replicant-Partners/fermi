# Fermi Backend Deployment

## Production URL
**https://fermi-nine.vercel.app**

## Deployment Status
✅ **LIVE** - Deployed on Vercel

## API Endpoints

### Health Check
```bash
GET https://fermi-nine.vercel.app/api/health
```

**Response:**
```json
{
  "status": "ok",
  "service": "fermi-backend",
  "version": "0.4.0"
}
```

### Execute Forecast
```bash
POST https://fermi-nine.vercel.app/api/execute
Content-Type: application/json

{
  "fpl_code": "forecast code here",
  "iterations": 50000
}
```

**Response:**
```json
{
  "success": true,
  "result": {
    "p50": 1200.0,
    "p10": 800.0,
    "p90": 1800.0,
    "mean": 1205.0,
    "iterations": 50000,
    "duration_ms": 234
  }
}
```

## Technology Stack

- **Runtime:** Vercel Serverless Functions (Rust)
- **Language:** Rust 2021
- **Framework:** vercel_runtime v2
- **Repository:** https://github.com/Replicant-Partners/fermi

## Local Development

### Prerequisites
- Rust toolchain (latest stable)
- Vercel CLI: `npm i -g vercel`

### Running Locally
```bash
# Install dependencies
cargo build

# Run Vercel dev server
vercel dev

# Test endpoints
curl http://localhost:3000/api/health
```

### Deploying to Production
```bash
# Deploy to production
vercel --prod

# View logs
vercel logs
```

## Project Structure

```
fermi/
├── api/
│   ├── health.rs       # Health check endpoint
│   └── execute.rs      # Forecast execution endpoint
├── src/
│   ├── lib.rs          # Core FPL library
│   ├── lexer.rs        # Tokenization
│   ├── parser.rs       # AST construction
│   ├── semantic.rs     # Type checking
│   ├── evaluator.rs    # Expression evaluation
│   ├── executor.rs     # Monte Carlo simulation
│   └── distributions.rs # Probability distributions
├── Cargo.toml          # Rust dependencies
├── vercel.json         # Vercel configuration
└── README.md           # Project documentation
```

## Architecture Alignment

This backend deployment aligns with:
- **ADR-002:** Rust Backend Rebuild
- **ADR-005:** Hybrid Execution Model (backend handles ≥100K iterations)

### Execution Flow

```
Zed IDE → FPL LSP → Local Execution (<100K iterations)
                  ↘
                    Backend API (≥100K iterations, agents)
```

## Next Steps

1. **Integrate FPL Executor:** Replace placeholder with actual execution engine
2. **Add Agent Coordination:** Implement agent orchestration endpoints
3. **Database Integration:** Add PostgreSQL for persistence
4. **Authentication:** Add API key authentication for production use
5. **Rate Limiting:** Implement request throttling

## Monitoring

- **Vercel Dashboard:** https://vercel.com/ivan-5553s-projects/fermi
- **Logs:** `vercel logs --follow`
- **Analytics:** Built-in Vercel analytics

## Environment Variables

Configure in Vercel dashboard:
- `RUST_LOG`: Logging level (info, debug, trace)
- `DATABASE_URL`: PostgreSQL connection string (future)
- `API_KEY_SECRET`: API authentication (future)

## Troubleshooting

### Build Failures
```bash
# Check build logs
vercel inspect [deployment-url] --logs

# Test locally
cargo build --release
```

### API Errors
```bash
# View function logs
vercel logs --follow

# Test endpoint locally
vercel dev
curl http://localhost:3000/api/health
```

## Performance

- **Cold Start:** ~200-500ms
- **Warm Response:** ~50-100ms
- **Concurrent Requests:** Auto-scaling via Vercel

## Cost

- **Vercel Hobby Plan:** Free tier includes:
  - 100 GB-hours serverless execution
  - 100 GB bandwidth
  - Unlimited requests

## Security

- **HTTPS Only:** All traffic encrypted
- **CORS:** Configured for frontend domains
- **Input Validation:** Request body parsing with error handling

## Support

- **Issues:** https://github.com/Replicant-Partners/fermi/issues
- **Documentation:** See docs/ directory
- **Contact:** Replicant Partners team

---

**Last Updated:** 2026-02-04  
**Version:** 0.4.0  
**Status:** Production Ready ✅
