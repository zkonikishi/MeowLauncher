export function mountContent(invoke,realm='s4a21'){
 const $=id=>document.getElementById(id);if(!$('content-items'))return;
 let snapshot=null,loadedAt=0,stale=true,busy=false,category='all',lastAttempt=0;
 function render(){
  $('content-items').replaceChildren();
  const items=(snapshot?.items||[]).filter(a=>a.realms.includes(realm)&&(category==='all'||a.category===category));
  if(!items.length){const p=document.createElement('p');p.textContent=snapshot?'该分类暂无资讯。':'官网资讯暂不可用。';$('content-items').append(p);}
  for(const item of items.slice(0,3)){const b=document.createElement('button');b.className='news-item';b.textContent=item.title;b.title=item.summary;b.onclick=()=>invoke('open_news',{url:item.url}).catch(()=>{$('content-state').textContent='无法打开官网，请稍后重试。';});$('content-items').append(b);}
  const m=snapshot?.maintenance;const age=m?Date.now()-Date.parse(m.updatedAt):Infinity;
  const fresh=!stale&&m?.realms.includes(realm)&&Number.isFinite(age)&&age>=-60000&&age<=300000;
  const labels={unknown:'维护状态未知',scheduled:'官网已发布维护计划',active:'官网提示维护中',resolved:'官网已发布维护结束通知',cancelled:'官网已取消维护计划'};
  $('maintenance-state').textContent=fresh?(labels[m.status]||'维护状态未知'):'维护状态未知，请查看官网';
 }
 async function refresh(){if(busy)return;busy=true;lastAttempt=Date.now();$('content-retry').disabled=true;$('content-state').textContent='正在加载官网资讯…';
  try{const data=await invoke('load_content');if(data.schemaVersion!==1||!Array.isArray(data.items)||!Number.isFinite(Date.parse(data.updatedAt)))throw Error('invalid');snapshot=data;loadedAt=Date.now();stale=false;$('content-state').textContent='官网更新 '+new Date(data.updatedAt).toLocaleString();}
  catch{stale=true;$('content-state').textContent=snapshot?'刷新失败 · 显示上次读取内容 '+new Date(loadedAt).toLocaleTimeString():'加载失败，请重试';}
  finally{busy=false;$('content-retry').disabled=false;render();}
 }
 $('content-retry').onclick=refresh;
 $('news-tabs').onclick=e=>{const b=e.target.closest('[data-category]');if(!b)return;category=b.dataset.category;document.querySelectorAll('#news-tabs button').forEach(x=>x.classList.toggle('selected',x===b));render();};
 const timer=setInterval(()=>{if(!document.hidden)refresh();},300000);
 document.addEventListener('visibilitychange',()=>{if(!document.hidden){render();if(Date.now()-lastAttempt>=300000)refresh();}});
 window.addEventListener('pagehide',()=>clearInterval(timer),{once:true});refresh();
}
