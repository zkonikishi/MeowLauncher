import fs from 'node:fs';
import path from 'node:path';
import {registerIpc} from './ipc.mjs';

// Isolated UI startup test. No credentials, account request or game invocation.
export async function run(context) {
  const {app, windows, version, outPath, dataDir} = context;
  if (!outPath || !process.env.MEOW_DATA_DIR) throw new Error('Explicit isolated data/output paths required');
  await app.whenReady();
  registerIpc(context);
  const win = await windows.create();
  let state;
  for(let i=0;i<40;i++) {
    state=await win.webContents.executeJavaScript(`(async()=>{
      const r=await window.meow.info();
      const logo=document.querySelector('.ol-logo .beta-brand-main');
      const tag=document.querySelector('.ol-logo .beta-brand-label');
      return {product:r.data?.productName,edition:r.data?.edition,website:r.data?.website,
        version:r.data?.version,login:!!document.querySelector('.ol-sidebar'),
        notice:document.querySelector('.snapshot-preview-notice')?.textContent,
        logoLoaded:!!logo?.complete && logo.naturalWidth>0,logoAlt:logo?.alt,
        betaLoaded:!!tag?.complete && tag.naturalWidth>0 && tag.alt==='Beta',
        title:document.title};
    })()`);
    if(state.login && state.logoLoaded && state.product) break;
    await new Promise(resolve=>setTimeout(resolve,100));
  }
  const checks=[
    {name:'product',ok:state.product==='MeowLauncher Beta'},
    {name:'version',ok:state.version==='26.3-snapshot-1'},
    {name:'edition',ok:state.edition==='beta'},
    {name:'homepage',ok:state.website==='https://github.com/zkonikishi/MeowLauncher'},
    {name:'login renders',ok:state.login},
    {name:'Mrad logo loaded',ok:state.logoLoaded && state.logoAlt==='Mrad'},
    {name:'Beta product label loaded',ok:state.betaLoaded},
    {name:'preview scope visible',ok:state.notice?.includes('尚未接通')===true},
    {name:'isolated profile',ok:dataDir===path.resolve(process.env.MEOW_DATA_DIR)},
  ];
  const screenshot=outPath.replace(/\.json$/i,'.png');
  await win.webContents.executeJavaScript(`document.fonts.ready.then(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))))`);
  await new Promise(resolve=>setTimeout(resolve,750));
  fs.mkdirSync(path.dirname(outPath),{recursive:true});
  fs.writeFileSync(screenshot,(await win.webContents.capturePage()).toPNG());
  const report={ok:checks.every(c=>c.ok),version:version.version,state,checks,screenshot,gameStarted:false,accountRequest:false};
  fs.writeFileSync(outPath,JSON.stringify(report,null,2));
  app.exit(report.ok?0:1);
}
