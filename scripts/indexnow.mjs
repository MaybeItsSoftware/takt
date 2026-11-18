#!/usr/bin/env node
// Tells Bing (and the other IndexNow engines) about every URL in the live
// sitemap of the site in site/. Run after the pages workflow deploys a change:
//   node scripts/indexnow.mjs
// The key is public by design; it must match site/<key>.txt.
const HOST = 'takt.maybeitssoftware.co.uk'
const KEY = 'f5b343957ab1629a53ef17f5c89082fc'
const ORIGIN = `https://${HOST}`

const sitemap = await fetch(`${ORIGIN}/sitemap.xml`).then(r => {
  if (!r.ok) throw new Error(`sitemap.xml: HTTP ${r.status}`)
  return r.text()
})
const urlList = [...sitemap.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)].map(m => m[1])
if (!urlList.length) throw new Error('No <loc> entries in sitemap.xml')

const res = await fetch('https://api.indexnow.org/indexnow', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json; charset=utf-8' },
  body: JSON.stringify({ host: HOST, key: KEY, keyLocation: `${ORIGIN}/${KEY}.txt`, urlList }),
})
console.log(`IndexNow: HTTP ${res.status} for ${urlList.length} URL(s)`)
for (const u of urlList) console.log(`  ${u}`)
if (res.status >= 400) process.exit(1)
