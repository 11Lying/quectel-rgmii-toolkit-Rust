const assert = require('node:assert/strict');

(async () => {
  const base = process.env.DEVICE_URL || 'http://127.0.0.1:18081';
  const login = await fetch(base+'/api/login', {method:'POST',body:new URLSearchParams({username:process.env.DEVICE_USER||'admin',password:process.env.DEVICE_PASSWORD||'admin'})});
  assert.equal(login.status,200);
  const cookie = login.headers.get('set-cookie').split(';')[0];
  async function request(endpoint, options={}) {
    const r = await fetch(base+endpoint, {...options, headers:{cookie,...options.headers}});
    assert.equal(r.status,200,endpoint);
    return r.json();
  }
  for (const endpoint of ['/api/dashboard_data','/api/device_info_data','/api/network_data']) {
    await request(endpoint);
  }
  const initial = await request('/api/telemetry');
  assert.equal(initial.ping,undefined);
  console.log('Read-only API checks passed. Waiting 310 seconds with no history requests.');
  await new Promise(resolve=>setTimeout(resolve,310000));
  const data = await request('/api/telemetry');
  assert(data.signal.length>=58 && data.signal.length<=60);
  assert(data.signal.every(p=>p.time>data.serverTime-300000));
  assert(data.traffic.length<=60);
  assert(data.traffic.every(p=>p.time>data.serverTime-300000));
  const interval = points => {const spans=points.slice(1).map((p,i)=>p.time-points[i].time).sort((a,b)=>a-b);return spans[Math.floor(spans.length/2)];};
  assert(Math.abs(interval(data.signal)-5000)<150);
  assert(data.signal.some(p=>p.rsrpNR!==null && p.sinrNR!==null && p.temperature!==null));
  console.log(JSON.stringify({signalPoints:data.signal.length,trafficPoints:data.traffic.length,signalIntervalMs:interval(data.signal),backgroundCollection:'passed'}));
})().catch(error=>{console.error(error);process.exitCode=1;});
