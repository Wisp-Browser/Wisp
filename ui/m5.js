// Wisp milestone 5 UI: command palette, shortcuts, find, suggestions, shield, library, settings, toasts.
(function(){
const $=s=>document.querySelector(s),send=o=>window.ipc.postMessage(JSON.stringify(o));
const esc=s=>String(s==null?'':s).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const I={
 shield:'<svg viewBox="0 0 16 16"><path d="M8 1.5l5.2 1.8v4.2c0 3.1-2.2 5.4-5.2 6.9C5 12.900 2.800 10.600 2.800 7.500V3.300z"/></svg>',
 star:'<svg viewBox="0 0 16 16"><path d="M8 1.800l1.900 3.900 4.300.6-3.100 3 .7 4.300L8 11.600 4.200 13.600l.7-4.300-3.100-3 4.300-.6z"/></svg>',
 dots:'<svg viewBox="0 0 16 16" class="fill"><circle cx="3.500" cy="8" r="1.300"/><circle cx="8" cy="8" r="1.300"/><circle cx="12.500" cy="8" r="1.300"/></svg>',
 up:'<svg viewBox="0 0 16 16"><path d="M4 10l4-4 4 4"/></svg>',down:'<svg viewBox="0 0 16 16"><path d="M4 6l4 4 4-4"/></svg>',
 x:'<svg viewBox="0 0 16 16"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
 search:'<svg viewBox="0 0 16 16"><circle cx="7" cy="7" r="4.200"/><path d="M10.200 10.200L13.500 13.500"/></svg>',
 clock:'<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="5.500"/><path d="M8 4.800V8l2.200 1.400"/></svg>',
 tab:'<svg viewBox="0 0 16 16"><rect x="2" y="3.500" width="12" height="9" rx="2"/><path d="M2 6.500h12"/></svg>',
 cmd:'<svg viewBox="0 0 16 16"><path d="M5 3a2 2 0 100 4h6a2 2 0 100-4 2 2 0 00-2 2v6a2 2 0 102-2H5a2 2 0 102 2V5a2 2 0 00-2-2z"/></svg>'};
let S=null,booted=false;
const ovs=new Set();
function ov(){}
const act=(t,x)=>send(Object.assign({t},x||{}));
// Anything the chrome draws over the page area is reported as a "hole": the page window is cut
// away there, so menus and dialogs appear on top of a page that stays put and stays visible.
let holeSig='';
function syncHoles(){requestAnimationFrame(()=>{const rects=[];
 [['#pop',0],['#sugg',0],['#palm',1],['#libm',1],['#setm',1],['#spmodal',1],['#ctx',0]].forEach(([sel,inner])=>{const e=$(sel);if(!e||!e.classList.contains('open'))return;
  const el=inner?e.querySelector('.box'):e,r=el.getBoundingClientRect(),rad=parseFloat(getComputedStyle(el).borderTopLeftRadius)||14;if(r.width>0&&r.height>0)rects.push({x:Math.round(r.left),y:Math.round(r.top),w:Math.round(r.width),h:Math.round(r.height),r:rad})});
 const sig=JSON.stringify(rects);if(sig!==holeSig){holeSig=sig;send({t:'holes',rects})}})}
function watchHoles(){const mo=new MutationObserver(syncHoles),ro=new ResizeObserver(syncHoles);
 ['#pop','#sugg','#palm','#libm','#setm','#spmodal'].forEach(sel=>{const e=$(sel);if(!e)return;mo.observe(e,{attributes:true,attributeFilter:['class','style']});ro.observe(e);const b=e.querySelector('.box');if(b)ro.observe(b)});
 addEventListener('resize',syncHoles)}
const refreshInset=syncHoles,inset=()=>{};
const cur=()=>S&&S.tabs.find(t=>t.id===S.active);

// ---------- commands + shortcuts ----------
const CMDS=[
 ['newtab','New tab','ctrl+t',()=>act('new')],['closetab','Close tab','ctrl+w',()=>act('close',{id:S.active})],
 ['reload','Reload page','ctrl+r',()=>act('reload')],['address','Focus address bar','ctrl+l',()=>{$('#url').focus()}],
 ['palette','Command palette','ctrl+k',()=>palette(true)],['ai','Toggle Wisp AI','ctrl+j',()=>window.AI&&AI.toggle()],
 ['find','Find in page','ctrl+f',()=>openFind()],['findnext','Find next','f3',()=>findStep('next')],['findprev','Find previous','shift+f3',()=>findStep('prev')],
 ['zoomin','Zoom in','ctrl+=',()=>act('zoom',{d:1})],['zoomout','Zoom out','ctrl+-',()=>act('zoom',{d:-1})],['zoomreset','Reset zoom','ctrl+0',()=>act('zoom',{d:0})],
 ['reader','Reader mode','ctrl+shift+r',()=>act('reader')],['bookmark','Bookmark this page','ctrl+d',()=>act('bm_toggle')],
 ['history','History','ctrl+h',()=>openLib('history')],['bookmarks','Bookmarks','ctrl+shift+o',()=>openLib('bookmarks')],
 ['downloads','Downloads','ctrl+shift+j',()=>openLib('downloads')],['settings','Settings','ctrl+,',()=>openSettings()],
 ['private','New private window','ctrl+shift+n',()=>act('private')],['fullscreen','Toggle fullscreen','f11',()=>act('fullscreen')],
 ['print','Print page','ctrl+p',()=>act('print')],['siteblock','Toggle ad blocker on this site','ctrl+shift+b',()=>act('site_blocker')],
 ['pin','Pin / unpin tab','ctrl+shift+p',()=>S&&act('pin',{id:S.active})],
 ['nexttab','Next tab','ctrl+tab',()=>stepTab(1)],['prevtab','Previous tab','ctrl+shift+tab',()=>stepTab(-1)],
 ['unsplit','Close split view','',()=>act('unsplit')],['theme','Switch light / dark','',()=>act('theme')],
 ['hibernate','Hibernate background tabs','',()=>{S.tabs.filter(t=>t.id!==S.active&&!(S.split&&S.split.includes(t.id))).forEach(t=>act('hibernate',{id:t.id}));toast('Background tabs hibernated')}],
 ['importbm','Import bookmarks…','',()=>openSettings('bookmarks')],
].map(([id,name,combo,run])=>({id,name,combo,run}));
for(let n=1;n<=9;n++)CMDS.push({id:'space'+n,name:'Go to space '+n,combo:'alt+'+n,run:()=>{const s=S.spaces[n-1];if(s)act('space_switch',{id:s.id})},hide:true});
const stepTab=d=>{if(!S||!S.tabs.length)return;const i=S.tabs.findIndex(t=>t.id===S.active),n=S.tabs[(i+d+S.tabs.length)%S.tabs.length];act('select',{id:n.id})};
const eff=c=>{const m=S&&S.s&&S.s.shortcuts;return m&&m[c.id]!==undefined?m[c.id]:c.combo};
const CODES={Equal:'=',Minus:'-',Comma:',',Period:'.',Slash:'/',Backslash:'\\',Backquote:'`',Space:'space',Escape:'escape',Tab:'tab',Enter:'enter',ArrowLeft:'left',ArrowRight:'right',ArrowUp:'up',ArrowDown:'down'};
const combo=e=>{const m=[];if(e.ctrlKey||e.metaKey)m.push('ctrl');if(e.altKey)m.push('alt');if(e.shiftKey)m.push('shift');
 const k=CODES[e.code]||(/^Key[A-Z]$/.test(e.code)?e.code[3].toLowerCase():/^Digit\d$/.test(e.code)?e.code[5]:/^F\d+$/.test(e.code)?e.code.toLowerCase():String(e.key).toLowerCase());
 if(/^(control|shift|alt|meta)$/.test(k))return '';m.push(k);return m.join('+')};
const pretty=c=>c?c.split('+').map(p=>({ctrl:'Ctrl',alt:'Alt',shift:'Shift',space:'Space',tab:'Tab',escape:'Esc'}[p]||(p.length===1?p.toUpperCase():p.toUpperCase()))).join(' + '):'Not set';
let combosKey='';
function pushCombos(){const l=CMDS.map(eff).filter(Boolean),k=l.join(',');if(k!==combosKey){combosKey=k;act('combos',{list:l})}}
function runCombo(c){const cmd=CMDS.find(x=>eff(x)===c);if(cmd){cmd.run();return true}return false}
let capturing=null;
addEventListener('keydown',e=>{
 if(capturing){e.preventDefault();e.stopPropagation();const c=combo(e);if(e.key==='Escape'){capturing=null;renderSettings();return}
  if(!c)return;setShortcut(capturing,e.key==='Backspace'||e.key==='Delete'?'':c);capturing=null;return}
 if(!(e.ctrlKey||e.metaKey||e.altKey||/^F\d+$/.test(e.code)))return;
 const c=combo(e);if(c&&runCombo(c)){e.preventDefault();e.stopPropagation()}},true);
function setShortcut(id,c){const m=Object.assign({},(S.s&&S.s.shortcuts)||{});
 if(c){CMDS.forEach(o=>{if(o.id!==id&&eff(o)===c){m[o.id]='';toast('Moved '+c+' from “'+o.name+'”')}})}
 m[id]=c;S.s.shortcuts=m;act('set',{k:'shortcuts',v:m});pushCombos();renderSettings()}

// ---------- toasts ----------
function stickyToast(text,btn,fn){const box=$('#toasts');if(!box)return;box.querySelectorAll('.toast.sticky').forEach(e=>e.remove());const t=document.createElement('div');t.className='toast sticky';
 t.innerHTML='<span></span><button class="pbtn go"></button>';t.firstChild.textContent=text;const b=t.querySelector('button');b.textContent=btn;b.onclick=fn;box.appendChild(t)}
function toast(text){const box=$('#toasts');if(!box||!text)return;const t=document.createElement('div');t.className='toast';t.textContent=text;box.appendChild(t);
 setTimeout(()=>t.classList.add('out'),3200);setTimeout(()=>t.remove(),3700)}

// ---------- build static UI ----------
function build(){
 const omni=$('#omni'),eng=$('#aibtn');
 const mk=(id,cls,html,title)=>{const b=document.createElement('button');b.id=id;b.className=cls;b.innerHTML=html;if(title)b.title=title;return b};
 eng.before(mk('zmchip','chip hidden','100%','Reset zoom'),mk('shbtn','ib2',I.shield+'<b id="shn"></b>','Ad & tracker blocker'),mk('bmbtn','ib2',I.star,'Bookmark this page (Ctrl+D)'));
 omni.after(mk('morebtn','nav',I.dots,'More'));
 $('#bar').insertAdjacentHTML('beforeend',`<div id="findbar" class="panel"><input id="fq" placeholder="Find in page" spellcheck="false"><span id="fcount"></span><button class="ib2" id="fprev">${I.up}</button><button class="ib2" id="fnext">${I.down}</button><button class="ib2" id="fx">${I.x}</button></div>`);
 document.body.insertAdjacentHTML('beforeend',`<div id="sugg" class="panel"></div><div id="pop" class="panel"></div>
  <div class="veil" id="palm"><div class="box big panel"><input id="pq" placeholder="Type a command, tab, or address…" autocomplete="off"><div id="pr"></div></div></div>
  <div class="veil" id="libm"><div class="box big panel xl"><div class="mh"><div class="seg" id="libtabs"></div><input id="libq" placeholder="Search"><button class="pbtn" id="libx">Close</button></div><div id="libbody"></div></div></div>
  <div class="veil" id="setm"><div class="box big panel xl setbox"><nav id="setnav"></nav><div id="setbody"></div></div></div>`);
 $('#side').insertAdjacentHTML('beforeend','<div id="toasts"></div>');
 $('#title .brand').insertAdjacentHTML('beforeend','<em id="privbadge" class="hidden">Private</em>');
 $('#shbtn').onclick=e=>{e.stopPropagation();popover('shield')};$('#bmbtn').onclick=()=>act('bm_toggle');$('#morebtn').onclick=e=>{e.stopPropagation();popover('more')};
 $('#zmchip').onclick=()=>act('zoom',{d:0});
 $('#fq').oninput=()=>findNow('fresh');$('#fq').onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();findStep(e.shiftKey?'prev':'next')}else if(e.key==='Escape')closeFind()};
 $('#fnext').onclick=()=>findStep('next');$('#fprev').onclick=()=>findStep('prev');$('#fx').onclick=closeFind;
 // palette
 $('#pq').oninput=()=>{pal.sel=0;renderPal()};$('#pq').onkeydown=palKey;$('#palm').onmousedown=e=>{if(e.target.id==='palm')palette(false)};
 // library + settings
 $('#libx').onclick=closeLib;$('#libm').onmousedown=e=>{if(e.target.id==='libm')closeLib()};$('#libq').oninput=()=>{lib.q=$('#libq').value;reqLib()};
 $('#setm').onmousedown=e=>{if(e.target.id==='setm')closeSettings()};
 addEventListener('click',e=>{if(!e.target.closest('#pop,#shbtn,#morebtn'))closePop();if(!e.target.closest('#sugg,#url'))closeSugg()});
 // suggestions on the address bar
 const u=$('#url');u.addEventListener('input',()=>requestSugg(),true);
 u.addEventListener('keydown',e=>{if(!sg.open)return;if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();e.stopImmediatePropagation();moveSugg(e.key==='ArrowDown'?1:-1)}
  else if(e.key==='Enter'&&sg.sel>=0){e.preventDefault();e.stopImmediatePropagation();pickSugg(sg.items[sg.sel]);u.blur()}else if(e.key==='Escape'){closeSugg();u.blur()}},true);
 u.addEventListener('blur',()=>setTimeout(closeSugg,150));
 watchHoles();booted=true}

// ---------- find in page ----------
let finding=false;
function openFind(){const b=$('#findbar');b.classList.add('open');finding=true;$('#fq').focus();$('#fq').select();if($('#fq').value)findNow('fresh')}
function closeFind(){$('#findbar').classList.remove('open');finding=false;act('find',{dir:'clear',q:''});$('#fcount').textContent=''}
const findNow=d=>act('find',{q:$('#fq').value,dir:d,cs:false});
const findStep=d=>{if(!finding)return openFind();findNow(d)};

// ---------- suggestions ----------
const sg={open:false,items:[],remote:[],n:0,sel:-1,q:''};let sgT=0;
function requestSugg(){const q=$('#url').value;sg.q=q;sg.sel=-1;clearTimeout(sgT);if(!q.trim()){closeSugg();return}
 sgT=setTimeout(()=>{sg.n++;sg.items=[];sg.remote=[];act('suggest',{q,n:sg.n})},90)}
function renderSugg(){const box=$('#sugg');const q=sg.q.trim();
 const rows=[...sg.items,...sg.remote.map(t=>({k:'search',title:t,url:''}))];
 if(q)rows.unshift({k:'search',title:q,url:'',first:true});
 const seen=new Set();sg.rows=rows.filter(r=>{const k=r.k+'|'+(r.url||r.title);if(seen.has(k))return false;seen.add(k);return true}).slice(0,9);
 if(!q||!sg.rows.length){closeSugg();return}
 sg.items2=sg.rows;box.innerHTML=sg.rows.map((r,i)=>`<div class="sr ${i===sg.sel?'sel':''}" data-i="${i}"><i>${r.k==='tab'?I.tab:r.k==='search'?I.search:I.clock}</i><span>${esc(r.title)}</span><small>${esc(r.k==='search'?'Search':r.k==='tab'?'Switch to tab':r.url)}</small></div>`).join('');
 box.classList.add('open');sg.open=true;refreshInset();
 box.querySelectorAll('.sr').forEach(el=>el.onmousedown=e=>{e.preventDefault();pickSugg(sg.rows[+el.dataset.i]);$('#url').blur()})}
function moveSugg(d){const n=sg.rows.length;sg.sel=(sg.sel+d+n+1)%(n+1)-1;renderSugg();if(sg.sel>=0){const r=sg.rows[sg.sel];$('#url').value=r.url||r.title}}
function pickSugg(r){if(!r)return;closeSugg();if(r.k==='tab')act('select',{id:r.id});else act('go',{input:r.url||r.title})}
function closeSugg(){if(!sg.open)return;sg.open=false;$('#sugg').classList.remove('open');refreshInset()}

// ---------- popovers (shield + more) ----------
let popKind=null;
function popover(kind){if(popKind===kind){closePop();return}popKind=kind;const p=$('#pop'),pg=(S&&S.page)||{};
 if(kind==='shield'){const f=S.filters||{};p.innerHTML=`<h4>${esc(pg.host||'This page')}</h4>
  <div class="prow2"><span>Ad & tracker blocking<br><small>${S.s.blocker?(pg.site_off?'Off on this site':'On'):'Off everywhere (Settings)'}</small></span><button class="sw2 ${S.s.blocker&&!pg.site_off?'on':''}" id="pst" ${pg.host&&S.s.blocker?'':'disabled'}><i></i></button></div>
  <div class="stat"><b>${pg.blocked||0}</b> blocked on this page<br><b>${f.total||0}</b> this session · ${f.rules||0} filter rules</div>
  <button class="pbtn wide" id="psl">Blocker settings</button>`;
  p.style.right=(innerWidth-($('#shbtn').getBoundingClientRect().right)-8+ (S.ai&&S.ai.open?0:0))+'px';
  setTimeout(()=>{$('#pst').onclick=()=>{closePop();act('site_blocker')};$('#psl').onclick=()=>{closePop();openSettings('privacy')}})}
 else{const items=[['find','Find in page'],['zoomin','Zoom in'],['zoomout','Zoom out'],['reader','Reader mode'],['bookmark','Bookmark this page'],['bookmarks','Bookmarks'],['history','History'],['downloads','Downloads'],['private','New private window'],['fullscreen','Fullscreen'],['print','Print…'],['settings','Settings']];
  p.innerHTML='<div class="menu">'+items.map(([id,n])=>{const c=CMDS.find(x=>x.id===id);return `<button data-c="${id}"><span>${n}</span><small>${esc(c&&eff(c)?pretty(eff(c)):'')}</small></button>`}).join('')+'</div>';
  p.style.right=(innerWidth-$('#morebtn').getBoundingClientRect().right)+'px';
  setTimeout(()=>p.querySelectorAll('[data-c]').forEach(b=>b.onclick=()=>{closePop();CMDS.find(x=>x.id===b.dataset.c).run()}))}
 p.classList.add('open');refreshInset()}
function closePop(){if(!popKind)return;popKind=null;$('#pop').classList.remove('open');refreshInset()}

// ---------- command palette ----------
const pal={sel:0,rows:[]};
function palette(open){const m=$('#palm');if(open===false){m.classList.remove('open');ov('pal',false);return}
 m.classList.add('open');ov('pal',true);$('#pq').value='';pal.sel=0;renderPal();$('#pq').focus()}
function score(h,q){h=h.toLowerCase();if(!q)return 1;if(h.startsWith(q))return 3;if(h.includes(q))return 2;let i=0;for(const c of h){if(c===q[i])i++;if(i===q.length)return 1}return 0}
function renderPal(){const q=$('#pq').value.trim().toLowerCase(),rows=[];
 S.tabs.forEach(t=>{const s=Math.max(score(t.title||'',q),score(t.url,q)*.9);if(s)rows.push({s:s+.5,k:'tab',name:t.title||'New Tab',sub:t.url==='wisp://newtab'?'':t.url,run:()=>act('select',{id:t.id})})});
 CMDS.filter(c=>!c.hide||q).forEach(c=>{const s=score(c.name,q);if(s)rows.push({s,k:'cmd',name:c.name,sub:pretty(eff(c)),run:()=>c.run()})});
 if(q&&!rows.some(r=>r.s>=2))rows.push({s:.1,k:'go',name:/\./.test(q)&&!/\s/.test(q)?'Go to '+q:'Search for “'+q+'”',sub:'',run:()=>act('go',{input:$('#pq').value.trim()})});
 pal.rows=rows.sort((a,b)=>b.s-a.s).slice(0,40);pal.sel=Math.min(pal.sel,Math.max(0,pal.rows.length-1));
 $('#pr').innerHTML=pal.rows.map((r,i)=>`<div class="prow ${i===pal.sel?'sel':''}" data-i="${i}"><i>${r.k==='tab'?I.tab:r.k==='go'?I.search:I.cmd}</i><span>${esc(r.name)}</span><small>${esc(r.sub)}</small></div>`).join('')||'<div class="prow"><span style="opacity:.6">No matches</span></div>';
 $('#pr').querySelectorAll('.prow[data-i]').forEach(el=>el.onclick=()=>{const r=pal.rows[+el.dataset.i];palette(false);r.run()});
 const s=$('#pr .sel');if(s)s.scrollIntoView({block:'nearest'})}
function palKey(e){if(e.key==='ArrowDown'){e.preventDefault();pal.sel=Math.min(pal.sel+1,pal.rows.length-1);renderPal()}else if(e.key==='ArrowUp'){e.preventDefault();pal.sel=Math.max(pal.sel-1,0);renderPal()}
 else if(e.key==='Enter'){e.preventDefault();const r=pal.rows[pal.sel];palette(false);if(r)r.run()}else if(e.key==='Escape')palette(false)}

// ---------- library (bookmarks / history / downloads) ----------
const lib={tab:'bookmarks',q:'',items:[]};
function openLib(tab){lib.tab=tab||lib.tab;lib.q='';$('#libq').value='';$('#libm').classList.add('open');ov('lib',true);
 $('#libtabs').innerHTML=[['bookmarks','Bookmarks'],['history','History'],['downloads','Downloads']].map(([k,n])=>`<button data-k="${k}" class="${lib.tab===k?'on':''}">${n}</button>`).join('');
 $('#libtabs').querySelectorAll('button').forEach(b=>b.onclick=()=>openLib(b.dataset.k));$('#libq').style.display=lib.tab==='downloads'?'none':'';reqLib()}
function closeLib(){$('#libm').classList.remove('open');ov('lib',false)}
const reqLib=()=>act('lib',{what:lib.tab,q:lib.q});
const ago=ts=>{const s=Date.now()/1000-ts;return s<90?'just now':s<5400?Math.round(s/60)+' min ago':s<172800?Math.round(s/3600)+' h ago':new Date(ts*1000).toLocaleDateString([],{month:'short',day:'numeric'})};
function renderLib(items){lib.items=items;const b=$('#libbody');
 const head=lib.tab==='history'?'<div class="lbar"><button class="pbtn" id="hclr">Clear all history</button></div>':lib.tab==='downloads'?'<div class="lbar"><button class="pbtn" id="dclr">Clear list</button></div>':'';
 if(!items.length){b.innerHTML=head+`<div class="none">${lib.tab==='bookmarks'?'No bookmarks yet. Press Ctrl+D on a page, or import them in Settings → Bookmarks.':lib.tab==='history'?'No history yet.':'No downloads yet.'}</div>`}
 else b.innerHTML=head+items.map((x,i)=>lib.tab==='downloads'
  ?`<div class="lr"><div class="lt"><b>${esc(x.name)}</b><small>${x.state==='progress'?'Downloading…':x.state==='failed'?'Failed':'Saved · '+ago(x.ts)}</small></div>${x.state==='done'?`<button class="pbtn" data-o="${x.id}">Open</button><button class="pbtn" data-s="${x.id}">Show in folder</button>`:''}</div>`
  :`<div class="lr go" data-u="${esc(x.url)}"><i>${lib.tab==='bookmarks'?I.star:I.clock}</i><div class="lt"><b>${esc(x.title||x.url)}</b><small>${esc(x.url)}</small></div>${x.folder?`<em>${esc(x.folder)}</em>`:''}${x.ts&&lib.tab==='history'?`<em>${ago(x.ts)}</em>`:''}<button class="ib2" data-d="${i}" title="Remove">${I.x}</button></div>`).join('');
 b.querySelectorAll('.lr.go').forEach(r=>r.onclick=e=>{if(e.target.closest('[data-d]'))return;closeLib();act('go',{input:r.dataset.u})});
 b.querySelectorAll('[data-d]').forEach(x=>x.onclick=()=>act(lib.tab==='bookmarks'?'bm_del':'hist_del',{url:lib.items[+x.dataset.d].url,q:lib.q}));
 b.querySelectorAll('[data-o]').forEach(x=>x.onclick=()=>act('dl_open',{id:+x.dataset.o}));b.querySelectorAll('[data-s]').forEach(x=>x.onclick=()=>act('dl_show',{id:+x.dataset.s}));
 const h=$('#hclr');if(h)h.onclick=()=>{act('clear',{what:['history']});setTimeout(reqLib,120)};const d=$('#dclr');if(d)d.onclick=()=>act('dl_clear')}

// ---------- settings ----------
let setSec='general',srcs=[],updMsg='';
function openSettings(sec){if(sec)setSec=sec;$('#setm').classList.add('open');ov('set',true);if(setSec==='bookmarks')act('sources');renderSettings()}
function closeSettings(){capturing=null;$('#setm').classList.remove('open');ov('set',false)}
const tg=(on,k)=>`<button class="sw2 ${on?'on':''}" data-set="${k}" role="switch"><i></i></button>`;
const row=(t,sub,ctl)=>`<div class="srow"><span>${t}${sub?`<small>${sub}</small>`:''}</span>${ctl||''}</div>`;
function renderSettings(){if(!S||!$('#setm').classList.contains('open'))return;const s=S.s,secs=[['general','General'],['privacy','Privacy & security'],['tabs','Tabs'],['shortcuts','Shortcuts'],['bookmarks','Bookmarks'],['updates','Updates'],['about','About']];
 $('#setnav').innerHTML='<h3>Settings</h3>'+secs.map(([k,n])=>`<button data-sec="${k}" class="${setSec===k?'on':''}">${n}</button>`).join('')+'<button class="pbtn" id="setx">Close</button>';
 let h='';
 if(setSec==='general')h=`<h2>General</h2>${row('Default search engine','Used by the address bar. Type a prefix like “d cats” to use another engine once.',`<div class="seg">${S.engines.map(e=>`<button data-eng="${e.id}" class="${S.engine===e.id?'on':''}">${esc(e.name)}</button>`).join('')}</div>`)}
  ${row('Search suggestions','Sends what you type in the address bar to your search engine to get suggestions.',tg(s.suggest,'suggest'))}
  ${row('Appearance','Wallpaper, widgets, accent colour and glass live in Customize on the new tab page.',`<button class="pbtn" data-act="theme">Light / Dark</button>`)}`;
 if(setSec==='privacy')h=`<h2>Privacy & security</h2>
  ${row('Ad & tracker blocker','Blocks ad and tracking requests using filter lists, and hides common ad boxes.',tg(s.blocker,'blocker'))}
  ${row('Filter lists',`${(S.filters&&S.filters.rules)||0} rules loaded · ${(S.filters&&S.filters.total)||0} requests blocked this session`,'<button class="pbtn" data-act="filters">Update lists</button>')}
  ${s.block_off.length?`<div class="sub"><b>Blocking is off on:</b>${s.block_off.map(h=>`<span class="chipx" data-un="${esc(h)}">${esc(h)} ${I.x}</span>`).join('')}</div>`:''}
  ${row('HTTPS-only mode','Upgrades http:// sites to https://. Local addresses and custom ports are left alone. A site without HTTPS will not load while this is on.',tg(s.https_only,'https_only'))}
  ${row('Camera, microphone & location','Sites ask for permission the first time and Windows remembers your choice per site.','')}
  <div class="sub"><b>Clear browsing data</b><div class="clr"><label><input type="checkbox" id="c_h" checked> Browsing history</label><label><input type="checkbox" id="c_c"> Cookies & site data</label><button class="pbtn" data-act="clear">Clear</button></div></div>`;
 if(setSec==='tabs')h=`<h2>Tabs</h2>${row('Hibernate inactive tabs','Frees memory by unloading tabs you haven’t used. They reload when you click them. Tabs playing audio or video are never hibernated.',`<div class="seg">${[[0,'Off'],[10,'10 min'],[20,'20 min'],[30,'30 min'],[60,'1 hour']].map(([v,n])=>`<button data-hib="${v}" class="${s.hibernate_min===v?'on':''}">${n}</button>`).join('')}</div>`)}
  ${row('Hibernate background tabs now','',`<button class="pbtn" data-c="hibernate">Hibernate now</button>`)}`;
 if(setSec==='shortcuts')h=`<h2>Keyboard shortcuts</h2><p class="hint">Click a shortcut, then press the new keys. Backspace clears it, Esc cancels.</p>`+CMDS.filter(c=>!c.hide).map(c=>`<div class="srow"><span>${esc(c.name)}</span><button class="key ${capturing===c.id?'cap':''}" data-key="${c.id}">${capturing===c.id?'Press keys…':esc(pretty(eff(c)))}</button></div>`).join('')+`<div class="srow"><span>Go to space 1–9</span><span class="key static">Alt + 1…9</span></div><button class="pbtn" data-act="resetkeys" style="margin-top:12px">Reset all shortcuts</button>`;
 if(setSec==='bookmarks')h=`<h2>Bookmarks</h2><p class="hint">Import your bookmarks from another browser. Nothing is deleted from it.</p>${srcs.map(x=>`<div class="srow"><span>${esc(x.label)}</span><button class="pbtn" data-imp="${x.id}">Import</button></div>`).join('')||'<div class="none sm">No other browsers found automatically.</div>'}
  ${row('From an HTML file','Works with exports from Firefox, Chrome, Edge and others.','<button class="pbtn" data-imp="html">Choose file…</button>')}
  ${row('Your bookmarks','',`<button class="pbtn" data-act="openbm">Open</button>`)}`;
 if(setSec==='updates')h=`<h2>Updates</h2>${row('Current version','Wisp '+esc(S.version),'')}
  ${S.update&&S.update.ready?row('Version '+esc(S.update.ready)+' is ready','Restart Wisp to finish installing it. Your tabs come back.','<button class="pbtn go" data-act="restart">Restart now</button>'):row('Check for updates','Looks for a newer release on GitHub.',`<button class="pbtn" data-act="upd">Check now</button>`)}${updMsg?`<div class="sub">${esc(updMsg)}</div>`:''}
  ${row('Update automatically','Checks every few hours, downloads in the background, and installs when you restart. It never interrupts your work.',tg(s.auto_update,'auto_update'))}
  ${row('Release channel','Beta gets new features first.',`<div class="seg">${['stable','beta'].map(c=>`<button data-ch="${c}" class="${s.channel===c?'on':''}">${c[0].toUpperCase()+c.slice(1)}</button>`).join('')}</div>`)}`;
 if(setSec==='about')h=`<h2>About Wisp</h2><div class="about"><img src="/logo.svg" alt=""><div><b>Wisp ${esc(S.version)}</b><small>A light, glassy browser built on WebView2.</small></div></div>${row('Source code','github.com/jakedev2/Wisp','<button class="pbtn" data-open="https://github.com/jakedev2/Wisp">Open</button>')}`;
 $('#setbody').innerHTML=h;
 const b=$('#setm');b.querySelectorAll('[data-sec]').forEach(x=>x.onclick=()=>{setSec=x.dataset.sec;capturing=null;if(setSec==='bookmarks')act('sources');renderSettings()});
 $('#setx').onclick=closeSettings;
 b.querySelectorAll('[data-set]').forEach(x=>x.onclick=()=>{act('set',{k:x.dataset.set,v:!s[x.dataset.set]})});
 b.querySelectorAll('[data-eng]').forEach(x=>x.onclick=()=>act('engine',{id:x.dataset.eng}));
 b.querySelectorAll('[data-hib]').forEach(x=>x.onclick=()=>act('set',{k:'hibernate_min',v:+x.dataset.hib}));
 b.querySelectorAll('[data-ch]').forEach(x=>x.onclick=()=>act('set',{k:'channel',v:x.dataset.ch}));
 b.querySelectorAll('[data-un]').forEach(x=>x.onclick=()=>act('set',{k:'unblock_site',v:x.dataset.un}));
 b.querySelectorAll('[data-imp]').forEach(x=>x.onclick=()=>act('import',{src:x.dataset.imp}));
 b.querySelectorAll('[data-open]').forEach(x=>x.onclick=()=>{closeSettings();act('open',{url:x.dataset.open})});
 b.querySelectorAll('[data-key]').forEach(x=>x.onclick=()=>{capturing=x.dataset.key;renderSettings()});
 b.querySelectorAll('[data-c]').forEach(x=>x.onclick=()=>CMDS.find(c=>c.id===x.dataset.c).run());
 b.querySelectorAll('[data-act]').forEach(x=>x.onclick=()=>{const a=x.dataset.act;
  if(a==='theme')act('theme');else if(a==='filters'){toast('Downloading filter lists…');act('filters_update')}
  else if(a==='upd'){updMsg='Checking…';renderSettings();act('upd_check')}else if(a==='restart')act('update_apply')
  else if(a==='clear'){const w=[];if($('#c_h').checked)w.push('history');if($('#c_c').checked)w.push('cookies');if(w.length)act('clear',{what:w})}
  else if(a==='resetkeys'){S.s.shortcuts={};act('set',{k:'shortcuts',v:{}});pushCombos();renderSettings()}
  else if(a==='openbm'){closeSettings();openLib('bookmarks')}})}

// ---------- state + events ----------
function state(s){S=s;if(!booted)build();
 document.body.classList.toggle('private',!!s.private);$('#privbadge').classList.toggle('hidden',!s.private);
 const pg=s.page||{};$('#bmbtn').classList.toggle('on',!!pg.bm);const n=pg.blocked||0,sh=$('#shn');sh.textContent=n>99?'99+':n||'';$('#shbtn').classList.toggle('active',!!n);$('#shbtn').classList.toggle('off',!s.s.blocker||!!pg.site_off);
 const z=$('#zmchip');z.classList.toggle('hidden',!pg.zoom||pg.zoom===1);z.textContent=Math.round((pg.zoom||1)*100)+'%';
 pushCombos();if($('#setm').classList.contains('open')&&!capturing)renderSettings()}
function event(e){switch(e.type){
 case'toast':toast(e.text);break;
 case'lib':if(e.what===lib.tab)renderLib(e.items);break;
 case'findcount':$('#fcount').textContent=$('#fq').value?(e.n?e.n+' found':'No matches'):'';break;
 case'sugg':if(e.n===sg.n){sg.items=e.items;renderSugg()}break;
 case'sugg_remote':if(e.n===sg.n){sg.remote=e.items;renderSugg()}break;
 case'sources':srcs=e.items;if(setSec==='bookmarks')renderSettings();break;
 case'upd':updMsg=e.msg;renderSettings();if(!$('#setm').classList.contains('open'))toast(e.msg);break;
 case'update_ready':updMsg='Version '+e.version+' is ready.';stickyToast('Update '+e.version+' ready',"Restart",()=>act('update_apply'));renderSettings();break;
 case'pageclick':closePop();closeSugg();if($('#palm').classList.contains('open'))palette(false);if($('#libm').classList.contains('open'))closeLib();if($('#setm').classList.contains('open')&&!capturing)closeSettings();if($('#spmodal')&&$('#spmodal').classList.contains('open'))$('#spmodal').classList.remove('open');break;
 case'imported':if($('#libm').classList.contains('open'))reqLib();break}}
window.M5={inset,state,event,key:c=>{if(c==='escape'&&S&&S.fullscreen){act('fullscreen');return}runCombo(c)},palette:()=>palette(true)};
})();
