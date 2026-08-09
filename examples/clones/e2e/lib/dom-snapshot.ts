import type { Page } from '@playwright/test'

// Per-step serialized-DOM snapshot for the interaction differential (v2).
//
// The serializer is dependency-free and injected via page.evaluate — the
// vite preview server doesn't serve tests/e2e/lib/serialize-dom.js, so the
// projection logic lives inline here (same semantics: stable JSON, sorted
// attributes, whitespace-normalized text, React bookkeeping attrs stripped).
//
// Differences from the tests/e2e serializer, needed for CROSS-TRACK diffs:
//  * serializes <body> (not #root) so portal-mounted UI — e.g. the netflix
//    detail modal rendered under document.body — is part of the projection;
//    script/style/link/noscript nodes are skipped (identical across tracks
//    anyway, they're the same built app).
//  * attribute values have the '/react-reference/' route prefix collapsed
//    to '/' so oracle-page hrefs compare equal to ps-page hrefs.
//  * live input state is captured as pseudo-attributes (@value/@checked) —
//    React does NOT reflect the value property back into the attribute, so
//    typed text would otherwise be invisible to a DOM diff. Elements whose
//    data-testid is listed in volatileValueTestIds (audio/video-driven range
//    inputs like seek bars) are exempt.

export interface SnapshotOpts {
  selector?: string
  volatileValueTestIds?: string[]
}

export async function domSnapshot(page: Page, opts: SnapshotOpts = {}): Promise<string> {
  return page.evaluate(
    ({ sel, volatile }) => {
      const SKIP = { SCRIPT: 1, STYLE: 1, LINK: 1, NOSCRIPT: 1 } as Record<string, 1>

      function ser(node: Node): unknown {
        if (node.nodeType === 3) {
          let t = (node.textContent || '').replace(/\s+/g, ' ').trim()
          // The vite page shells label some tracks in an <h1> —
          // "Twitter (PythScribe, production)" vs "Twitter (React reference)".
          // That's a shell label, not clone output; normalize it away.
          t = t.replace(/\s*\((?:PythScribe[^)]*|React reference)\)$/, '')
          return t ? { t } : null
        }
        if (node.nodeType !== 1) return null
        const el = node as HTMLElement
        if (SKIP[el.tagName]) return null

        const out: { e: string; a?: Record<string, string>; c?: unknown[] } = {
          e: el.tagName.toLowerCase(),
        }
        const tid = el.getAttribute('data-testid') || ''
        const isVolatile = volatile.indexOf(tid) !== -1

        const names: string[] = []
        for (let i = 0; i < el.attributes.length; i++) names.push(el.attributes[i].name)
        names.sort()
        const attrs: Record<string, string> = {}
        for (const n of names) {
          if (n.indexOf('data-react') === 0) continue
          // Volatile elements (seek bars / media elements bound to real
          // media clocks or track-divergent assets): value/max are
          // media-time-derived and src can be deliberately different per
          // track (the /spotify wrapper injects real mp3s that the oracle
          // wrapper does not — demo-only divergence). Drop all three.
          if (isVolatile && (n === 'value' || n === 'max' || n === 'src')) continue
          let v = el.getAttribute(n) || ''
          v = v.split('/react-reference/').join('/')
          attrs[n] = v
        }

        const tag = el.tagName
        if ((tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') && !isVolatile) {
          const iel = el as HTMLInputElement
          attrs['@value'] = String(iel.value)
          if (tag === 'INPUT' && (iel.type === 'checkbox' || iel.type === 'radio')) {
            attrs['@checked'] = String(iel.checked)
          }
        }
        if (Object.keys(attrs).length) out.a = attrs

        // Volatile subtrees (elapsed/total time labels bound to real media
        // clocks) keep their element shell but drop their content.
        if (isVolatile) {
          out.c = [{ t: '<volatile>' }]
          return out
        }

        const kids: unknown[] = []
        for (let i = 0; i < el.childNodes.length; i++) {
          const s = ser(el.childNodes[i])
          if (s) kids.push(s)
        }
        if (kids.length) out.c = kids
        return out
      }

      const root = document.querySelector(sel)
      if (!root) return JSON.stringify({ error: 'no root matching ' + sel })
      return JSON.stringify(ser(root))
    },
    { sel: opts.selector ?? 'body', volatile: opts.volatileValueTestIds ?? [] },
  )
}

// First differing JSON path between two snapshot strings — for actionable
// failure messages ("step 4: first diff at c[2].c[0].a.@value").
export function firstDiff(aStr: string, bStr: string): { path: string; a: unknown; b: unknown } | null {
  if (aStr === bStr) return null
  let a: unknown
  let b: unknown
  try {
    a = JSON.parse(aStr)
    b = JSON.parse(bStr)
  } catch {
    return { path: '(unparseable)', a: aStr.slice(0, 200), b: bStr.slice(0, 200) }
  }

  function walk(x: unknown, y: unknown, path: string): { path: string; a: unknown; b: unknown } | null {
    if (x === y) return null
    if (typeof x !== typeof y || x === null || y === null || typeof x !== 'object') {
      return { path: path || '(root)', a: x, b: y }
    }
    const xo = x as Record<string, unknown>
    const yo = y as Record<string, unknown>
    const keys = [...new Set([...Object.keys(xo), ...Object.keys(yo)])]
    // stable order for arrays and objects alike
    keys.sort((k1, k2) => (isNaN(+k1) || isNaN(+k2) ? k1.localeCompare(k2) : +k1 - +k2))
    for (const k of keys) {
      const sub = walk(xo[k], yo[k], path ? `${path}.${k}` : k)
      if (sub) return sub
    }
    return null
  }
  return walk(a, b, '') ?? { path: '(equal after parse?)', a: null, b: null }
}
