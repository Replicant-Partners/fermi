// Micro-chart — lightweight SVG sparklines and histograms
const MicroChart = {
  // Sparkline: array of values → inline SVG
  sparkline(values, opts = {}) {
    const w = opts.width || 200;
    const h = opts.height || 40;
    const color = opts.color || 'var(--green)';
    const dotColor = opts.dotColor || null;
    const dots = opts.dots || []; // indices to highlight as dots

    // `viewBox` + `preserveAspectRatio="none"` so CSS can set a width and the
    // path scales with it. Without a viewBox the width/height attributes are the
    // coordinate system, so `width:100%` scaled the viewport and left the path
    // drawn at its original size — a 200px sparkline in a 150px grid cell was
    // clipped at the tile border rather than fitted to it.
    //
    // The attributes stay as intrinsic defaults, so every existing caller that
    // does not override width in CSS renders exactly as before.
    const box = `viewBox="0 0 ${w} ${h}" preserveAspectRatio="none"`;

    if (!values || values.length < 2) return `<svg width="${w}" height="${h}" ${box}></svg>`;

    const max = Math.max(...values, 1);
    const min = Math.min(...values, 0);
    const range = max - min || 1;
    const step = w / (values.length - 1);
    const pad = 4;
    const ih = h - pad * 2;

    const points = values.map((v, i) => {
      const x = i * step;
      const y = pad + ih - ((v - min) / range) * ih;
      return { x, y, v, i };
    });

    const pathD = points.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');

    // Area fill
    const areaD = pathD + ` L${points[points.length-1].x.toFixed(1)},${h} L0,${h} Z`;

    let svg = `<svg width="${w}" height="${h}" ${box} style="display:block">`;
    svg += `<path d="${areaD}" fill="${color}" opacity="0.1" />`;

    // Optional reference line: a level the series must be read against rather
    // than in isolation — e.g. the base-rate baseline below which a Brier
    // score carries no information however high it looks. Drawn beneath the
    // series so it can never hide a data point, and clipped out silently when
    // it falls outside the plotted range.
    if (opts.refLine != null) {
      const ry = pad + ih - ((opts.refLine - min) / range) * ih;
      if (ry >= 0 && ry <= h) {
        svg += `<line x1="0" y1="${ry.toFixed(1)}" x2="${w}" y2="${ry.toFixed(1)}" `
             + `stroke="${opts.refColor || 'var(--fg4)'}" stroke-width="1" stroke-dasharray="2,2" />`;
      }
    }

    svg += `<path d="${pathD}" fill="none" stroke="${color}" stroke-width="1.5" />`;

    // Highlight dots (e.g. regression flags)
    dots.forEach(idx => {
      if (idx >= 0 && idx < points.length) {
        const p = points[idx];
        const dc = dotColor || 'var(--red)';
        svg += `<circle cx="${p.x.toFixed(1)}" cy="${p.y.toFixed(1)}" r="3" fill="${dc}" />`;
      }
    });

    // End dot
    const last = points[points.length - 1];
    svg += `<circle cx="${last.x.toFixed(1)}" cy="${last.y.toFixed(1)}" r="2.5" fill="${color}" />`;

    svg += '</svg>';
    return svg;
  },

  // Histogram: array of {label, value} or just values → SVG bar chart
  histogram(data, opts = {}) {
    const w = opts.width || 200;
    const h = opts.height || 60;
    const barColor = opts.color || 'var(--aqua)';

    if (!data || data.length === 0) return `<svg width="${w}" height="${h}"></svg>`;

    const items = data.map(d => typeof d === 'number' ? { label: '', value: d } : d);
    const max = Math.max(...items.map(d => d.value), 1);
    const barW = Math.min((w - (items.length - 1) * 2) / items.length, 40);
    const gap = 2;
    const totalW = items.length * barW + (items.length - 1) * gap;
    const offsetX = (w - totalW) / 2;
    const labelH = 14;
    const barH = h - labelH;

    let svg = `<svg width="${w}" height="${h}" style="display:block">`;

    items.forEach((d, i) => {
      const bh = (d.value / max) * (barH - 4);
      const x = offsetX + i * (barW + gap);
      const y = barH - bh;

      svg += `<rect x="${x.toFixed(1)}" y="${y.toFixed(1)}" width="${barW.toFixed(1)}" height="${bh.toFixed(1)}" fill="${barColor}" rx="1" opacity="0.85" />`;
      svg += `<text x="${(x + barW / 2).toFixed(1)}" y="${(h - 1).toFixed(1)}" text-anchor="middle" fill="var(--fg3)" font-size="9" font-family="inherit">${d.label}</text>`;

      // Value label on top of bar
      if (d.value > 0) {
        svg += `<text x="${(x + barW / 2).toFixed(1)}" y="${(y - 2).toFixed(1)}" text-anchor="middle" fill="var(--fg3)" font-size="8" font-family="inherit">${d.value}</text>`;
      }
    });

    svg += '</svg>';
    return svg;
  }
};
