// Wisp milestone 5 UI: command palette, shortcuts, find, suggestions, shield, library, settings, toasts.
(function(){
const $=s=>document.querySelector(s),send=o=>window.ipc.postMessage(JSON.stringify(o));
const esc=s=>String(s==null?'':s).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const I=new Proxy({},{get:(_,k)=>ico(k)});
let S=null,booted=false;
const ovs=new Set();
function ov(){}
const act=(t,x)=>send(Object.assign({t},x||{}));
// Anything the chrome draws over the page area is reported as a "hole": the page window is cut
// away there, so menus and dialogs appear on top of a page that stays put and stays visible.
// Rects come from layout boxes (offset*), which ignore the scale/slide-in transforms of open animations.
let holeSig='',holeRaf=0;
const HOLES=[['#pop',0],['#sugg',0],['#ctx',0],['#spm',0],['#findbar',0],['#palm',1],['#libm',1],['#setm',1],['#spmodal',1],['#extm',1],['#pwbar',0]];
function lbox(el){let x=0,y=0,e=el;while(e){x+=e.offsetLeft;y+=e.offsetTop;e=e.offsetParent}return{x,y,w:el.offsetWidth,h:el.offsetHeight}}
function syncHoles(){if(holeRaf)return;holeRaf=requestAnimationFrame(()=>{holeRaf=0;const rects=[];
 const add=el=>{const b=lbox(el),rad=parseFloat(getComputedStyle(el).borderTopLeftRadius)||14;if(b.w>0&&b.h>0)rects.push({x:Math.round(b.x),y:Math.round(b.y),w:Math.round(b.w),h:Math.round(b.h),r:Math.min(rad,b.h/2)})};
 HOLES.forEach(([sel,inner])=>{const e=$(sel);if(!e||!e.classList.contains('open'))return;add(inner?e.querySelector('.box'):e)});
 document.querySelectorAll('#toasts .toast').forEach(add);
 const sig=JSON.stringify(rects);if(sig!==holeSig){holeSig=sig;send({t:'holes',rects})}})}
// Clicks that land on the chrome's bare background (e.g. a popup's shadow over the page) count as page clicks.
document.addEventListener('mousedown',e=>{if(e.target===document.body&&holeSig&&holeSig!=='[]')event({type:'pageclick'})});
function watchHoles(){const mo=new MutationObserver(syncHoles),ro=new ResizeObserver(syncHoles);
 HOLES.forEach(([sel])=>{const e=$(sel);if(!e)return;mo.observe(e,{attributes:true,attributeFilter:['class','style']});ro.observe(e);const b=e.querySelector('.box');if(b)ro.observe(b)});
 mo.observe($('#toasts'),{childList:true});addEventListener('resize',syncHoles)}
const refreshInset=syncHoles,inset=()=>{};
const cur=()=>S&&S.tabs.find(t=>t.id===S.active);
// Select the address bar, with the keyboard (Wisp moves it here from the page first).
// Pages being shown or hidden can take the keyboard back for a moment, so check again briefly.
function focusUrl(){const u=$('#url'),grab=()=>{act('chrome_focus');u.focus();u.select()};grab();
 [80,250,500,900].forEach(ms=>setTimeout(()=>{if(document.activeElement===u&&!document.hasFocus()&&!u.value)grab()},ms))}

// ---------- commands + shortcuts ----------
// Shortcuts follow Chrome's. A command can have several keys, separated by "|"; the first is the
// one shown in menus.
const CMDS=[
 ['newtab','New tab','ctrl+t',()=>act('new')],['closetab','Close tab','ctrl+w|ctrl+f4',()=>act('close',{id:S.active})],
 ['reopen','Reopen closed tab','ctrl+shift+t',()=>act('reopen')],
 ['reload','Reload page','ctrl+r|f5',()=>act('reload')],['hardreload','Reload, ignoring the cache','ctrl+shift+r|shift+f5|ctrl+f5',()=>act('hard_reload')],
 ['back','Back','alt+left',()=>act('back')],['fwd','Forward','alt+right',()=>act('fwd')],['home','Open the New Tab page here','alt+home',()=>act('home')],
 ['address','Focus address bar','ctrl+l|alt+d|f6|ctrl+e',()=>focusUrl()],
 ['palette','Command palette and tab search','ctrl+k|ctrl+shift+a',()=>palette(true)],['ai','Toggle Wisp AI','ctrl+shift+space',()=>window.AI&&AI.toggle()],
 ['find','Find in page','ctrl+f',()=>openFind()],['findnext','Find next','f3|ctrl+g',()=>findStep('next')],['findprev','Find previous','shift+f3|ctrl+shift+g',()=>findStep('prev')],
 ['zoomin','Zoom in','ctrl+=|ctrl+shift+=',()=>act('zoom',{d:1})],['zoomout','Zoom out','ctrl+-',()=>act('zoom',{d:-1})],['zoomreset','Reset zoom','ctrl+0',()=>act('zoom',{d:0})],
 ['reader','Reader mode','f9',()=>act('reader')],['bookmark','Bookmark this page','ctrl+d',()=>act('bm_toggle')],['bmall','Bookmark all tabs','ctrl+shift+d',()=>act('bm_all')],
 ['history','History','ctrl+h',()=>openLib('history')],['bmbar','Show or hide the bookmarks bar','ctrl+shift+b',()=>act('set',{k:'bmbar',v:!S.bmbar})],['bookmarks','Bookmarks','ctrl+shift+o',()=>openLib('bookmarks')],
 ['downloads','Downloads','ctrl+j',()=>openLib('downloads')],['extensions','Extensions','',()=>act('ext_manage')],['passwords','Passwords','',()=>act('pw_manage')],['webstore','Chrome Web Store','',()=>act('ext_store',{q:''})],['settings','Settings','ctrl+,',()=>openSettings()],
 ['clear','Clear browsing data','ctrl+shift+delete',()=>openSettings('privacy')],
 ['menu','Open the menu','alt+f|alt+e',()=>popover('more')],
 ['private','New private window','ctrl+shift+n',()=>act('private')],['fullscreen','Toggle fullscreen','f11',()=>act('fullscreen')],
 ['print','Print page','ctrl+p',()=>act('print')],['source','View page source','ctrl+u',()=>act('view_source')],
 ['devtools','Developer tools','f12|ctrl+shift+i',()=>act('devtools')],
 ['siteblock','Toggle ad blocker on this site','',()=>act('site_blocker')],
 ['pin','Pin / unpin tab','ctrl+shift+p',()=>S&&act('pin',{id:S.active})],
 ['nexttab','Next tab','ctrl+tab|ctrl+pagedown',()=>stepTab(1)],['prevtab','Previous tab','ctrl+shift+tab|ctrl+pageup',()=>stepTab(-1)],
 ['closewin','Close window','ctrl+shift+w',()=>act('win',{op:'close'})],
 ['unsplit','Close split view','',()=>act('unsplit')],['theme','Switch light / dark','',()=>act('theme')],
 ['hibernate','Hibernate background tabs','',()=>{S.tabs.filter(t=>t.id!==S.active&&!(S.split&&S.split.includes(t.id))).forEach(t=>act('hibernate',{id:t.id}));toast('Background tabs hibernated')}],
 ['importbm','Import bookmarks and history…','',()=>openSettings('bookmarks')],
].map(([id,name,combo,run])=>({id,name,combo,run}));
for(let n=1;n<=9;n++)CMDS.push({id:'space'+n,name:'Go to space '+n,combo:'alt+'+n,run:()=>{const s=S.spaces[n-1];if(s)act('space_switch',{id:s.id})},hide:true});
// Like Chrome: Ctrl+1…8 go to that tab, Ctrl+9 to the last one.
for(let n=1;n<=9;n++)CMDS.push({id:'tab'+n,name:n<9?'Go to tab '+n:'Go to the last tab',combo:'ctrl+'+n,run:()=>{const t=n<9?S.tabs[n-1]:S.tabs[S.tabs.length-1];if(t)act('select',{id:t.id})},hide:true});
const stepTab=d=>{if(!S||!S.tabs.length)return;const i=S.tabs.findIndex(t=>t.id===S.active),n=S.tabs[(i+d+S.tabs.length)%S.tabs.length];act('select',{id:n.id})};
const eff=c=>{const m=S&&S.s&&S.s.shortcuts;return m&&m[c.id]!==undefined?m[c.id]:c.combo};
const CODES={Equal:'=',Minus:'-',Comma:',',Period:'.',Slash:'/',Backslash:'\\',Backquote:'`',Space:'space',Escape:'escape',Tab:'tab',Enter:'enter',ArrowLeft:'left',ArrowRight:'right',ArrowUp:'up',ArrowDown:'down'};
const combo=e=>{const m=[];if(e.ctrlKey||e.metaKey)m.push('ctrl');if(e.altKey)m.push('alt');if(e.shiftKey)m.push('shift');
 const k=CODES[e.code]||(/^Key[A-Z]$/.test(e.code)?e.code[3].toLowerCase():/^Digit\d$/.test(e.code)?e.code[5]:/^F\d+$/.test(e.code)?e.code.toLowerCase():String(e.key).toLowerCase());
 if(/^(control|shift|alt|meta)$/.test(k))return '';m.push(k);return m.join('+')};
const keysOf=c=>(eff(c)||'').split('|').filter(Boolean);
const pretty1=c=>c?c.split('+').map(p=>({ctrl:'Ctrl',alt:'Alt',shift:'Shift',space:'Space',tab:'Tab',escape:'Esc'}[p]||({pagedown:'Page Down',pageup:'Page Up',delete:'Delete',home:'Home',left:'←',right:'→'}[p]||p.toUpperCase()))).join(' + '):'Not set';
// All keys of a shortcut ("Ctrl + R or F5"), or just the first for menus.
const pretty=c=>c?c.split('|').filter(Boolean).map(pretty1).join(' or '):'Not set';
const prettyFirst=c=>pretty1((c||'').split('|')[0]);
let combosKey='';
function pushCombos(){const l=CMDS.flatMap(keysOf),k=l.join(',');if(k!==combosKey){combosKey=k;act('combos',{list:l})}}
function runCombo(c){const cmd=CMDS.find(x=>keysOf(x).includes(c));if(cmd){cmd.run();return true}return false}
let capturing=null;
addEventListener('keydown',e=>{
 if(capturing){e.preventDefault();e.stopPropagation();const c=combo(e);if(e.key==='Escape'){capturing=null;renderSettings();return}
  if(!c)return;setShortcut(capturing,e.key==='Backspace'||e.key==='Delete'?'':c);capturing=null;return}
 if(!(e.ctrlKey||e.metaKey||e.altKey||/^F\d+$/.test(e.code)))return;
 const c=combo(e);if(c&&runCombo(c)){e.preventDefault();e.stopPropagation()}},true);
function setShortcut(id,c){const m=Object.assign({},(S.s&&S.s.shortcuts)||{});
 if(c){CMDS.forEach(o=>{const k=keysOf(o);if(o.id!==id&&k.includes(c)){m[o.id]=k.filter(x=>x!==c).join('|');toast('Moved '+pretty1(c)+' from “'+o.name+'”')}})}
 m[id]=c;S.s.shortcuts=m;act('set',{k:'shortcuts',v:m});pushCombos();renderSettings()}

// ---------- toasts ----------
function stickyToast(text,btn,fn){const box=$('#toasts');if(!box)return;box.querySelectorAll('.toast.sticky').forEach(e=>e.remove());const t=document.createElement('div');t.className='toast sticky';
 t.innerHTML='<span></span><button class="pbtn go"></button>';t.firstChild.textContent=text;const b=t.querySelector('button');b.textContent=btn;b.onclick=fn;box.appendChild(t)}
function toast(text){const box=$('#toasts');if(!box||!text)return;const t=document.createElement('div');t.className='toast';t.textContent=text;box.appendChild(t);
 setTimeout(()=>t.classList.add('out'),3200);setTimeout(()=>t.remove(),3700)}

// ---------- build static UI ----------
function build(){
 $('#shbtn').onclick=e=>{e.stopPropagation();popover('shield')};$('#bmbtn').onclick=()=>act('bm_toggle');$('#morebtn').onclick=e=>{e.stopPropagation();popover('more')};
 $('#zmchip').onclick=()=>act('zoom',{d:0});
 $('#extbtn').onclick=e=>{e.stopPropagation();popover('ext')};
 $('#extm').onmousedown=e=>{if(e.target.id==='extm')extAnswer(false)};
 // A click anywhere in Wisp's interface closes an open extension popup (pins toggle it themselves).
 document.addEventListener('mousedown',e=>{if(extPopup&&!e.target.closest('#extpins'))act('ext_popup_close')},true);
 $('#fq').oninput=()=>findNow('fresh');$('#fq').onkeydown=e=>{if(e.key==='Enter'){e.preventDefault();findStep(e.shiftKey?'prev':'next')}else if(e.key==='Escape')closeFind()};
 $('#fnext').onclick=()=>findStep('next');$('#fprev').onclick=()=>findStep('prev');$('#fx').onclick=closeFind;
 // palette
 $('#pq').oninput=()=>{pal.sel=0;renderPal()};$('#pq').onkeydown=palKey;$('#palm').onmousedown=e=>{if(e.target.id==='palm')palette(false)};
 // library + settings
 $('#libx').onclick=closeLib;$('#libm').onmousedown=e=>{if(e.target.id==='libm')closeLib()};$('#libq').oninput=()=>{lib.q=$('#libq').value;reqLib()};
 $('#setm').onmousedown=e=>{if(e.target.id==='setm')closeSettings()};
 wireSetDrag();
 addEventListener('keydown',e=>{if(e.key!=='Escape'||capturing)return;if($('#extm').classList.contains('open'))extAnswer(false);else if(popKind)closePop();else if($('#libm').classList.contains('open'))closeLib();else if($('#setm').classList.contains('open'))closeSettings()});
 addEventListener('click',e=>{if(!e.target.closest('#pop,#shbtn,#morebtn,#extbtn'))closePop();if(!e.target.closest('#sugg,#url'))closeSugg()});
 // suggestions on the address bar
 const u=$('#url');u.addEventListener('input',()=>requestSugg(),true);
 u.addEventListener('keydown',e=>{if(!sg.open)return;if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();e.stopImmediatePropagation();moveSugg(e.key==='ArrowDown'?1:-1)}
  else if(e.key==='Enter'&&sg.sel>=0){e.preventDefault();e.stopImmediatePropagation();pickSugg(sg.rows[sg.sel]);u.blur()}else if(e.key==='Escape')closeSugg()},true);
 u.addEventListener('blur',()=>setTimeout(closeSugg,150));
 watchHoles();booted=true}
// Popups that hang off a control follow it when the layout changes.
function relayout(){if(!booted)return;if(sg.open)placeSugg();if(popKind){const k=popKind;popKind=null;popover(k)}syncHoles()}
function placeSugg(){const r=$('#omni').getBoundingClientRect(),b=$('#sugg');b.style.left=Math.max(8,r.left-4)+'px';b.style.width=Math.max(320,r.width+8)+'px';b.style.top=(r.bottom+6)+'px'}

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
 placeSugg();box.classList.add('open');sg.open=true;refreshInset();
 box.querySelectorAll('.sr').forEach(el=>el.onmousedown=e=>{e.preventDefault();pickSugg(sg.rows[+el.dataset.i]);$('#url').blur()})}
function moveSugg(d){const n=sg.rows.length;sg.sel=(sg.sel+d+n+1)%(n+1)-1;renderSugg();if(sg.sel>=0){const r=sg.rows[sg.sel];$('#url').value=r.url||r.title}}
function pickSugg(r){if(!r)return;closeSugg();if(r.k==='tab')act('select',{id:r.id});else act('go',{input:r.url||r.title})}
function closeSugg(){if(!sg.open)return;sg.open=false;$('#sugg').classList.remove('open');refreshInset()}

// ---------- popovers (shield + more) ----------
let popKind=null;
function anchor(p,btn){const r=$(btn).getBoundingClientRect();p.style.top=(r.bottom+8)+'px';p.style.right=Math.max(8,innerWidth-r.right)+'px';p.style.left='auto'}
const MORE=[['find','search','Find in page'],['zoomin','zoomin','Zoom in'],['zoomout','zoomout','Zoom out'],['reader','reader','Reader mode'],['bookmark','star','Bookmark this page'],0,['bookmarks','bookmark','Bookmarks'],['history','history','History'],['downloads','download','Downloads'],['extensions','puzzle','Extensions'],['passwords','key','Passwords'],0,['private','private','New private window'],['fullscreen','full','Full screen'],['print','print','Print…'],['theme','contrast','Light / dark theme'],0,['settings','settings','Settings']];
function popover(kind){if(popKind===kind){closePop();return}popoverPre(kind);popKind=kind;const p=$('#pop'),pg=(S&&S.page)||{};
 if(kind==='shield'){const f=S.filters||{};p.innerHTML=`<h4>${esc(pg.host||'This page')}</h4>
  <div class="prow2"><span>Ad &amp; tracker blocking<br><small>${S.s.blocker?(pg.site_off?'Off on this site':'On'):'Off everywhere (Settings)'}</small></span><button class="sw2 ${S.s.blocker&&!pg.site_off?'on':''}" id="pst" role="switch" ${pg.host&&S.s.blocker?'':'disabled'}><i></i></button></div>
  <div class="stat"><b>${pg.blocked||0}</b> blocked on this page<br><b>${f.total||0}</b> this session · ${f.rules||0} filter rules</div>
  <button class="pbtn wide" id="psl">Blocker settings</button>`;
  anchor(p,'#shbtn');
  setTimeout(()=>{$('#pst').onclick=()=>{closePop();act('site_blocker')};$('#psl').onclick=()=>{closePop();openSettings('privacy')}})}
 else if(kind==='ext'){const xs=(S.exts||[]);
  p.innerHTML='<h4>Extensions</h4><div class="menu">'+(xs.length?xs.map(x=>`<div class="xrow"><button data-x="${esc(x.id)}" ${x.enabled?'':'style="opacity:.5"'} title="${esc(x.enabled?(x.popup?'Open':x.options?'Options':x.name):'Turned off')}">${xIcon(x)}<span>${esc(x.name)}</span></button><button class="xp ${x.pinned?'on':''}" data-xp="${esc(x.id)}" title="${x.pinned?'Unpin from toolbar':'Pin to toolbar'}">${ico('pin')}</button></div>`).join(''):'<div class="xnone">No extensions yet. Get them from the Chrome Web Store — they work just like in Chrome.</div>')
   +'<div class="msep"></div><button data-xa="store">'+ico('store')+'<span>Chrome Web Store</span></button><button data-xa="manage">'+ico('settings')+'<span>Manage extensions</span></button></div>';
  p.classList.add('xpop');anchor(p,'#extbtn');
  setTimeout(()=>{p.querySelectorAll('[data-x]').forEach(b=>b.onclick=()=>{const x=xs.find(y=>y.id===b.dataset.x);closePop();if(!x)return;if(!x.enabled){act('ext_manage');return}const r=$('#extbtn').getBoundingClientRect();act('ext_popup',{id:x.id,x:r.right,y:r.bottom+6})});
   p.querySelectorAll('[data-xp]').forEach(b=>b.onclick=e=>{e.stopPropagation();act('ext_pin',{id:b.dataset.xp})});
   p.querySelectorAll('[data-xa]').forEach(b=>b.onclick=()=>{closePop();act(b.dataset.xa==='store'?'ext_store':'ext_manage',{q:''})})})}
 else{p.innerHTML='<div class="menu">'+MORE.map(x=>{if(!x)return '<div class="msep"></div>';const c=CMDS.find(y=>y.id===x[0]);return `<button data-c="${x[0]}">${ico(x[1])}<span>${x[2]}</span><small>${esc(c&&eff(c)?prettyFirst(eff(c)):'')}</small></button>`}).join('')+'</div>';
  anchor(p,'#morebtn');
  setTimeout(()=>p.querySelectorAll('[data-c]').forEach(b=>b.onclick=()=>{closePop();CMDS.find(x=>x.id===b.dataset.c).run()}))}
 p.classList.add('open');refreshInset()}
function closePop(){if(!popKind)return;popKind=null;$('#pop').classList.remove('open','xpop');refreshInset()}
function popoverPre(kind){if(kind!=='ext')$('#pop').classList.remove('xpop')}
// ---------- extensions ----------
let extPopup=null,extAsk=null,extSig='';
const xIcon=x=>x.icon?`<img src="${esc(x.icon)}" alt="" onerror="this.outerHTML='<i class=xl0>${esc((x.name||'?')[0].toUpperCase())}</i>'">`:`<i class="xl0">${esc((x.name||'?')[0].toUpperCase())}</i>`;
let pinSig='';
function renderPins(){const xs=(S.exts||[]).filter(x=>x.pinned&&x.enabled),sig=JSON.stringify(xs.map(x=>[x.id,x.icon,x.name]))+extPopup;if(sig===pinSig)return;pinSig=sig;
 const box=$('#extpins');box.innerHTML=xs.map(x=>`<button class="ib ${extPopup===x.id?'on':''}" data-pin="${esc(x.id)}" title="${esc(x.name)}">${xIcon(x)}</button>`).join('');
 box.querySelectorAll('[data-pin]').forEach(b=>b.onclick=e=>{e.stopPropagation();closePop();const r=b.getBoundingClientRect();act('ext_popup',{id:b.dataset.pin,x:r.right,y:r.bottom+6})});
 $('#extbtn').classList.toggle('on',!!extPopup&&!xs.some(x=>x.id===extPopup))}
function extConfirm(e){extAsk=e.id;const b=$('#extbox');
 b.innerHTML=`<div class="xh">${e.icon?`<img src="${esc(e.icon)}" alt="">`:`<i class="xl0">${esc((e.name||'?')[0].toUpperCase())}</i>`}<h3>${e.update?'Update':'Add'} “${esc(e.name)}”?</h3></div>`
  +(e.warnings&&e.warnings.length?`<p>${e.update?'The new version asks for more access. It can:':'It can:'}</p><ul>${e.warnings.map(w=>`<li>${esc(w)}</li>`).join('')}</ul>`:'<p>It doesn’t ask for any special access.</p>')
  +`<div class="row2"><button class="pbtn" id="xno">Cancel</button><button class="pbtn go" id="xyes">${e.update?'Update':'Add extension'}</button></div>`;
 $('#extm').classList.add('open');$('#xno').onclick=()=>extAnswer(false);$('#xyes').onclick=()=>extAnswer(true);$('#xyes').focus()}
// ---------- "Save password?" ----------
let pwT=0;
function pwOffer(e){const b=$('#pwbar'),r=$('#row2').offsetHeight?$('#row2').getBoundingClientRect():$('#row1').getBoundingClientRect();b.style.top=(r.bottom+6)+'px';
 b.innerHTML=`<div class="ph"><i class="ki">${ico('key')}</i><div style="min-width:0"><b>${e.update?'Update':'Save'} password for ${esc(e.site)}?</b><small>${esc(e.user||'No username')} · ••••••••</small></div></div>
  <div class="row2">${e.update?'':'<button class="pbtn" data-pa="never">Never</button>'}<button class="pbtn" data-pa="dismiss">Not now</button><button class="pbtn go" data-pa="save">${e.update?'Update':'Save'}</button></div>`;
 b.querySelectorAll('[data-pa]').forEach(x=>x.onclick=()=>pwAnswer(x.dataset.pa));b.classList.add('open');clearTimeout(pwT);pwT=setTimeout(()=>pwAnswer('dismiss'),45000)}
function pwAnswer(a){const b=$('#pwbar');if(!b.classList.contains('open'))return;clearTimeout(pwT);b.classList.remove('open');act('pw_answer',{a})}
function extAnswer(ok){if(!extAsk)return;act('ext_confirm',{id:extAsk,ok});extAsk=null;$('#extm').classList.remove('open')}

// ---------- command palette ----------
const pal={sel:0,rows:[]};
function palette(open){const m=$('#palm');if(open===false){m.classList.remove('open');ov('pal',false);return}
 m.classList.add('open');ov('pal',true);$('#pq').value='';pal.sel=0;renderPal();$('#pq').focus()}
function score(h,q){h=h.toLowerCase();if(!q)return 1;if(h.startsWith(q))return 3;if(h.includes(q))return 2;let i=0;for(const c of h){if(c===q[i])i++;if(i===q.length)return 1}return 0}
function renderPal(){const q=$('#pq').value.trim().toLowerCase(),rows=[];
 S.tabs.forEach(t=>{const s=Math.max(score(t.title||'',q),score(t.url,q)*.9);if(s)rows.push({s:s+.5,k:'tab',name:t.title||'New Tab',sub:t.url==='wisp://newtab'?'':t.url,run:()=>act('select',{id:t.id})})});
 CMDS.filter(c=>!c.hide||q).forEach(c=>{const s=score(c.name,q);if(s)rows.push({s,k:'cmd',name:c.name,sub:eff(c)?prettyFirst(eff(c)):'',run:()=>c.run()})});
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
 const head=lib.tab==='bookmarks'?'<div class="lbar"><button class="pbtn" id="bnf">New folder</button></div>':lib.tab==='history'?'<div class="lbar"><button class="pbtn" id="hclr">Clear all history</button></div>':lib.tab==='downloads'?'<div class="lbar"><button class="pbtn" id="dclr">Clear list</button></div>':'';
 if(!items.length){b.innerHTML=head+`<div class="none">${lib.tab==='bookmarks'?'No bookmarks yet. Press Ctrl+D on a page, or import them in Settings → Bookmarks.':lib.tab==='history'?'No history yet.':'No downloads yet.'}</div>`}
 else b.innerHTML=head+items.map((x,i)=>lib.tab==='downloads'
  ?`<div class="lr"><i>${I.file}</i><div class="lt"><b>${esc(x.name)}</b><small>${x.state==='progress'?'Downloading…':x.state==='failed'?'Failed':'Saved · '+ago(x.ts)}</small></div>${x.state==='done'?`<button class="pbtn" data-o="${x.id}">Open</button><button class="pbtn" data-s="${x.id}">Show in folder</button>`:''}</div>`
  :`<div class="lr go" data-u="${esc(x.url)}"><i>${lib.tab==='bookmarks'?I.star:I.clock}</i><div class="lt"><b>${esc(x.title||x.url)}</b><small>${esc(x.url)}</small></div>${x.folder?`<em>${esc(x.folder)}</em>`:''}${x.ts&&lib.tab==='history'?`<em>${ago(x.ts)}</em>`:''}<button class="ib" data-d="${i}" title="Remove">${I.x}</button></div>`).join('');
 b.querySelectorAll('.lr.go').forEach(r=>r.onclick=e=>{if(e.target.closest('[data-d]'))return;closeLib();act('go',{input:r.dataset.u})});
 b.querySelectorAll('[data-d]').forEach(x=>x.onclick=()=>act(lib.tab==='bookmarks'?'bm_del':'hist_del',{url:lib.items[+x.dataset.d].url,q:lib.q}));
 b.querySelectorAll('[data-o]').forEach(x=>x.onclick=()=>act('dl_open',{id:+x.dataset.o}));b.querySelectorAll('[data-s]').forEach(x=>x.onclick=()=>act('dl_show',{id:+x.dataset.s}));
 const nf=$('#bnf');if(nf)nf.onclick=()=>{const n=prompt('Folder name');if(n&&n.trim())act('bm_folder_new',{name:n.trim()})};
 const h=$('#hclr');if(h)h.onclick=()=>{act('clear',{what:['history']});setTimeout(reqLib,120)};const d=$('#dclr');if(d)d.onclick=()=>act('dl_clear')}

// ---------- settings ----------
let setSec='general',srcs=[],updMsg='',setHtml='',setNavHtml='',setSecShown='';
// The Settings window can be dragged by its title, a section heading or empty space in its sidebar
// (double-click one of those to put it back in the middle). It is moved with left/top, not a
// transform, so the page cut-out behind it (see HOLES) follows it exactly.
let setOff={x:0,y:0};
function placeSet(){const b=$('#setm .box');b.style.position='relative';b.style.left=setOff.x+'px';b.style.top=setOff.y+'px';syncHoles()}
// Keep at least the title area on screen.
function clampSet(){const b=$('#setm .box');if(!b.offsetWidth)return;const l=b.offsetLeft-setOff.x,t=b.offsetTop-setOff.y,w=b.offsetWidth;
 setOff.x=Math.max(120-l-w,Math.min(innerWidth-l-120,setOff.x));setOff.y=Math.max(-t+4,Math.min(innerHeight-t-56,setOff.y))}
function wireSetDrag(){const m=$('#setm'),b=m.querySelector('.box');
 const handle=t=>!!(t.closest('#setnav h3,#setbody h2')||t.id==='setnav'||t===b);
 b.addEventListener('pointerdown',e=>{if(e.button!==0||!handle(e.target))return;e.preventDefault();
  const sx=e.clientX,sy=e.clientY,ox=setOff.x,oy=setOff.y;b.setPointerCapture(e.pointerId);b.classList.add('dragging');
  const mv=ev=>{setOff={x:ox+ev.clientX-sx,y:oy+ev.clientY-sy};clampSet();placeSet()};
  const up=()=>{b.removeEventListener('pointermove',mv);b.removeEventListener('pointerup',up);b.removeEventListener('pointercancel',up);b.classList.remove('dragging')};
  b.addEventListener('pointermove',mv);b.addEventListener('pointerup',up);b.addEventListener('pointercancel',up)});
 b.addEventListener('dblclick',e=>{if(handle(e.target)){setOff={x:0,y:0};placeSet()}});
 addEventListener('resize',()=>{if(m.classList.contains('open')){clampSet();placeSet()}})}
function openSettings(sec){if(sec)setSec=sec;$('#setm').classList.add('open');ov('set',true);if(setSec==='bookmarks')act('sources');renderSettings()}
function closeSettings(){capturing=null;$('#setm').classList.remove('open');ov('set',false);setHtml='';setNavHtml=''}
// ---------- appearance: shared by Settings and the first-run setup ----------
const LAYS=[['classic','Classic','Tabs on top, address bar on a row below'],['compact','Compact','Tabs and address bar in one slim row'],['vertical','Vertical','Collapsible tab sidebar, slim toolbar on top']];
const MINI={
 classic:[['','left:8px;top:7px;width:24%;height:8px'],['ac','left:calc(12% + 2px);top:16px;width:8px;height:2px'],['','left:calc(24% + 12px);top:7px;width:20%;height:8px'],['','left:calc(44% + 16px);top:7px;width:20%;height:8px'],['om','left:8px;right:8px;top:21px;height:9px'],['pg','left:6px;right:6px;top:35px;bottom:6px']],
 compact:[['om','left:8px;top:8px;width:34%;height:10px'],['','left:calc(34% + 14px);top:9px;width:17%;height:8px'],['ac','left:calc(42% + 10px);top:18px;width:8px;height:2px'],['','left:calc(51% + 18px);top:9px;width:17%;height:8px'],['','left:calc(68% + 22px);top:9px;width:14%;height:8px'],['pg','left:6px;right:6px;top:25px;bottom:6px']],
 vertical:[['om','left:30%;width:46%;top:7px;height:8px'],['pn','left:6px;top:21px;width:24%;bottom:6px;border-radius:5px'],['','left:12px;top:27px;width:15%;height:6px'],['ac','left:9px;top:27px;width:2px;height:6px'],['','left:12px;top:37px;width:15%;height:6px'],['','left:12px;top:47px;width:11%;height:6px'],['pg','left:calc(24% + 12px);top:21px;right:6px;bottom:6px']],
 glass:[['pn','left:6px;right:6px;top:6px;height:14px;border-radius:7px'],['pn','left:6px;top:26px;width:28%;bottom:6px;border-radius:6px'],['pn','left:calc(28% + 12px);top:26px;right:6px;bottom:6px;border-radius:6px']]};
const mini=(k,cls)=>`<div class="mini ${cls||''}">${MINI[k].map(([c,st])=>`<i class="${c}" style="${st}"></i>`).join('')}</div>`;
const layNow=()=>(S&&S.layout)||'classic',glassOk=()=>!!(S&&S.glass_ok),glassNow=()=>!!(S&&S.glass_on&&S.glass_ok);
const themeSeg=()=>[[0,'Dark'],[1,'Light']].map(([v,n])=>`<button data-theme="${v}" class="${!!(S&&S.light)===!!v?'on':''}">${n}</button>`).join('');
const layCards=()=>LAYS.map(([k,n,d])=>`<button class="pv ${layNow()===k?'on':''}" data-lay="${k}" aria-pressed="${layNow()===k}">${mini(k)}<b>${ico('check')}${n}</b><small>${d}</small></button>`).join('');
const glassCards=()=>`<button class="pv ${glassNow()?'on':''}" data-glass="1" ${glassOk()?'':'disabled'} aria-pressed="${glassNow()}">${mini('glass','glassy')}<b>${ico('check')}Glass</b><small>Frosted, see-through surfaces with soft depth</small></button>
 <button class="pv ${glassNow()?'':'on'}" data-glass="0" aria-pressed="${!glassNow()}">${mini('glass','flatty')}<b>${ico('check')}Flat</b><small>Solid surfaces, no blur or transparency</small></button>`;
const glassNote=()=>glassOk()?'':`<div class="note">${ico('info')}<span>This version of Windows (or its “Transparency effects” setting) doesn’t support window blur, so Wisp uses Flat. Glass turns on by itself when it becomes available.</span></div>`;
const FONTS=[['inter','Inter','Clean and friendly, like Apple’s system font','"Wisp Inter"'],['rounded','Nunito','Soft and rounded','"Wisp Nunito"'],['system','Segoe UI','The Windows system font','"Segoe UI Variable Text","Segoe UI"'],['serif','Serif','Classic and bookish','Charter,Cambria,Georgia,serif'],['mono','Mono','Monospaced, for a technical look','"Cascadia Code",Consolas,monospace']];
const fontNow=()=>(S&&S.font)||'inter';
const fontCards=()=>FONTS.map(([k,n,d,st])=>`<button class="pv fnt ${fontNow()===k?'on':''}" data-font="${k}" aria-pressed="${fontNow()===k}"><div class="mini fs" style="font-family:${st.replace(/"/g,'&quot;')}"><span>Aa</span><em>The quick brown fox</em></div><b>${ico('check')}${n}</b><small>${d}</small></button>`).join('');
function wireLook(root){
 root.querySelectorAll('[data-font]').forEach(x=>x.onclick=()=>{if(fontNow()!==x.dataset.font)act('set',{k:'font',v:x.dataset.font})});
 root.querySelectorAll('[data-lay]').forEach(x=>x.onclick=()=>{if(layNow()!==x.dataset.lay)act('set',{k:'layout',v:x.dataset.lay})});
 root.querySelectorAll('[data-glass]').forEach(x=>x.onclick=()=>act('set',{k:'glass_on',v:x.dataset.glass==='1'}));
 root.querySelectorAll('[data-theme]').forEach(x=>x.onclick=()=>{if(!!S.light!==(x.dataset.theme==='1'))act('theme')})}
// ---------- first-run setup ----------
const su={open:false,step:0,shown:-1,done:false};
function openSetup(){su.open=true;su.step=0;su.shown=-1;$('#setup').classList.add('open');act('overlay',{who:'setup',open:true});renderSetup()}
function finishSetup(){su.open=false;su.done=true;$('#setup').classList.remove('open');act('set',{k:'setup_done',v:true});act('overlay',{who:'setup',open:false})}
function renderSetup(){if(!su.open)return;const b=$('#setupbox');let h='';
 if(su.step===0)h=`<div class="su-h"><img src="/logo.svg" alt=""><h2>Welcome to Wisp</h2><p>Let’s set up how Wisp looks. You can change any of this later in Settings → Appearance.</p></div><div class="su-seg"><div class="seg">${themeSeg()}</div></div>`;
 if(su.step===1)h=`<div class="su-h"><h2>Choose a layout</h2><p>Every layout has the same features, just arranged differently.</p></div><div class="pvs">${layCards()}</div>`;
 if(su.step===2)h=`<div class="su-h"><h2>Glass effects</h2><p>Frosted, see-through surfaces, or solid and flat. Everything else stays the same.</p></div><div class="pvs">${glassCards()}</div>${glassNote()}`;
 if(su.step===3)h=`<div class="su-h"><h2>Pick a font</h2><p>Used across Wisp’s menus, tabs and pages.</p></div><div class="pvs">${fontCards()}</div>`;
 const anim=su.shown!==su.step;su.shown=su.step;
 b.innerHTML=`<div class="${anim?'su-body':''}">${h}</div><div class="su-f"><div class="su-dots">${[0,1,2,3].map(i=>`<i class="${i===su.step?'on':''}"></i>`).join('')}</div>${su.step?'<button class="pbtn" id="suback">Back</button>':''}<button class="pbtn go" id="sunext">${su.step<3?'Continue':'Start browsing'}</button></div>`;
 wireLook(b);$('#sunext').onclick=()=>{if(su.step<3){su.step++;renderSetup()}else finishSetup()};const bk=$('#suback');if(bk)bk.onclick=()=>{su.step--;renderSetup()};
 if(anim)$('#sunext').focus()}
const tg=(on,k)=>`<button class="sw2 ${on?'on':''}" data-set="${k}" role="switch"><i></i></button>`;
const row=(t,sub,ctl)=>`<div class="srow"><span>${t}${sub?`<small>${sub}</small>`:''}</span>${ctl||''}</div>`;
function renderSettings(){if(!S||!$('#setm').classList.contains('open'))return;const s=S.s,secs=[['general','General','globe'],['appearance','Appearance','palette'],['privacy','Privacy & security','shield'],['tabs','Tabs','tab'],['shortcuts','Shortcuts','cmd'],['bookmarks','Bookmarks & import','bookmark'],['extensions','Extensions','puzzle'],['passwords','Passwords','key'],['updates','Updates','download'],['about','About','info']];
 const nav='<h3>Settings</h3>'+secs.map(([k,n,ic])=>`<button data-sec="${k}" class="${setSec===k?'on':''}">${ico(ic)}${n}</button>`).join('')+'<button class="pbtn" id="setx">Close</button>';
 let h='';
 if(setSec==='general')h=`<h2>General</h2>${row('Default search engine','Used by the address bar. Type a prefix like “d cats” to use another engine once.',`<div class="seg">${S.engines.map(e=>`<button data-eng="${e.id}" class="${S.engine===e.id?'on':''}">${esc(e.name)}</button>`).join('')}</div>`)}
  ${row('Search suggestions','Sends what you type in the address bar to your search engine to get suggestions.',tg(s.suggest,'suggest'))}
  ${row('Appearance','Theme, layout, glass effects and font.',`<button class="pbtn" data-sec="appearance">Open</button>`)}`;
 if(setSec==='appearance')h=`<h2>Appearance</h2>
  <h5>Theme</h5><div class="seg" style="display:inline-flex">${themeSeg()}</div>
  <h5>Layout</h5><div class="pvs">${layCards()}</div>
  <h5>Glass effects</h5><div class="pvs">${glassCards()}</div>${glassNote()}
  <h5>Font</h5><div class="pvs">${fontCards()}</div>
  ${row('Bookmarks bar','Your bookmarks under the address bar. Shown once you have at least one bookmark.',`<button class="sw2 ${S.bmbar?'on':''}" id="bmbartg" role="switch"><i></i></button>`)}
  <h5>Wallpaper</h5>${row('Wallpaper, accent colour and widgets','Set these in Customize on the New Tab page. They are saved per space.',`<button class="pbtn" data-act="newtab">Open a New Tab</button>`)}`;
 if(setSec==='privacy')h=`<h2>Privacy & security</h2>
  ${row('Ad & tracker blocker','Blocks ad and tracking requests using filter lists, and hides common ad boxes.',tg(s.blocker,'blocker'))}
  ${row('Filter lists',`${(S.filters&&S.filters.rules)||0} rules loaded · ${(S.filters&&S.filters.total)||0} requests blocked this session`,'<button class="pbtn" data-act="filters">Update lists</button>')}
  ${s.block_off.length?`<div class="sub"><b>Blocking is off on:</b>${s.block_off.map(h=>`<span class="chipx" data-un="${esc(h)}">${esc(h)} ${I.x}</span>`).join('')}</div>`:''}
  ${row('HTTPS-only mode','Upgrades http:// sites to https://. Local addresses and custom ports are left alone. A site without HTTPS will not load while this is on.',tg(s.https_only,'https_only'))}
  ${row('Camera, microphone & location','Sites ask for permission the first time and Windows remembers your choice per site.','')}
  <div class="sub"><b>Clear browsing data</b><div class="clr"><label><input type="checkbox" id="c_h" checked> Browsing history</label><label><input type="checkbox" id="c_c"> Cookies & site data</label><button class="pbtn" data-act="clear">Clear</button></div></div>`;
 if(setSec==='tabs')h=`<h2>Tabs</h2>${row('Memory saver','Unloads tabs you aren’t looking at so Wisp stays light. They reload when you click them, at the same scroll position. Tabs playing audio or video, or with text typed into a form, are kept.',`<div class="seg">${[[30,'Maximum'],[300,'Balanced'],[1800,'Moderate'],[0,'Off']].map(([v,n])=>`<button data-hib="${v}" class="${s.hibernate_s===v?'on':''}" title="${v?'After '+(v<60?v+' seconds':v/60+' minutes')+' in the background':'Never unload tabs'}">${n}</button>`).join('')}</div>`)}
  ${row('Hibernate background tabs now','',`<button class="pbtn" data-c="hibernate">Hibernate now</button>`)}`;
 if(setSec==='shortcuts')h=`<h2>Keyboard shortcuts</h2><p class="hint">Click a shortcut, then press the new keys. Backspace clears it, Esc cancels.</p>`+CMDS.filter(c=>!c.hide).map(c=>`<div class="srow"><span>${esc(c.name)}</span><button class="key ${capturing===c.id?'cap':''}" data-key="${c.id}">${capturing===c.id?'Press keys…':esc(pretty(eff(c)))}</button></div>`).join('')+`<div class="srow"><span>Go to tab 1–8, or the last tab</span><span class="key static">Ctrl + 1…9</span></div><div class="srow"><span>Go to space 1–9</span><span class="key static">Alt + 1…9</span></div><button class="pbtn" data-act="resetkeys" style="margin-top:12px">Reset all shortcuts</button>`;
 if(setSec==='bookmarks')h=`<h2>Bookmarks &amp; import</h2><p class="hint">Bring your bookmarks and history over from another browser. Nothing is changed in that browser.</p>${srcs.map(x=>`<div class="srow"><span>${esc(x.label)}</span><span style="display:flex;gap:6px"><button class="pbtn" data-imp="${x.id}">Bookmarks</button><button class="pbtn" data-imp="hist:${x.id}">History</button></span></div>`).join('')||'<div class="none sm">No other browsers found automatically.</div>'}
  ${row('From an HTML file','Bookmarks exported from Chrome, Firefox, Edge and others, or from Google Takeout (Chrome → Bookmarks.html).','<button class="pbtn" data-imp="html">Choose file…</button>')}
  ${row('From Google Takeout','Your Chrome history from your Google account: at takeout.google.com export “Chrome”, unzip it and pick Chrome/History.json.','<button class="pbtn" data-imp="takeout">Choose file…</button>')}
  ${row('Your bookmarks','',`<button class="pbtn" data-act="openbm">Open</button>`)}`;
 if(setSec==='extensions'){const xs=S.exts||[];h=`<h2>Extensions</h2><p class="hint">Wisp runs Chrome extensions from the Chrome Web Store on the same engine as Microsoft Edge, so they work the way they do in Chrome.</p>
  ${row('Chrome Web Store','Browse and add extensions. Open any extension there and press “Add to Wisp”.','<button class="pbtn go" data-xs="store">Open store</button>')}
  ${row('Manage extensions','Turn extensions on or off, open their options, update or remove them.','<button class="pbtn" data-xs="manage">Open</button>')}
  ${xs.length?'<h5>Installed</h5>'+xs.map(x=>row(`<span style="display:inline-flex;align-items:center;gap:10px">${xIcon(x)}${esc(x.name)}</span>`,esc('Version '+x.version),`<span style="display:flex;gap:6px;align-items:center">${x.options?`<button class="pbtn" data-xo="${esc(x.id)}">Options</button>`:''}<button class="sw2 ${x.enabled?'on':''}" data-xt="${esc(x.id)}" role="switch"><i></i></button></span>`)).join(''):'<div class="none sm">No extensions installed yet.</div>'}`}
 if(setSec==='passwords'){const pw=S.pw||{};h=`<h2>Passwords</h2><p class="hint">Wisp keeps saved passwords encrypted for your Windows account. Showing or exporting them asks Windows Hello first.</p>
  ${row('Offer to save passwords','When you sign in to a site, Wisp asks whether to save the password, and fills it next time.',`<button class="sw2 ${pw.save?'on':''}" id="pwtg" role="switch"><i></i></button>`)}
  ${row('Saved passwords',(pw.count||0)+' saved','<button class="pbtn" data-pw="manage">Manage</button>')}
  <h5>Import</h5>${(pw.sources||[]).map(x=>row(esc(x.name),'Saved passwords from '+esc(x.name)+' on this PC.',`<button class="pbtn" data-pwi="${esc(x.id)}">Import</button>`)).join('')}
  ${row('From a file','A passwords CSV exported from any browser or password manager (Firefox, Safari, Bitwarden, 1Password and others).','<button class="pbtn" data-pwi="csv">Choose file…</button>')}
  ${row('Passkeys','Passkeys can’t be copied out of other browsers. Sites still let you sign in with them using Windows Hello or your phone.','<button class="pbtn" data-pw="passkeys">Windows passkeys</button>')}`}
 if(setSec==='updates')h=`<h2>Updates</h2>${row('Current version','Wisp '+esc(S.version),'')}
  ${S.update&&S.update.ready?row('Version '+esc(S.update.ready)+' is ready','Restart Wisp to finish installing it. Your tabs come back.','<button class="pbtn go" data-act="restart">Restart now</button>'):row('Check for updates','Looks for a newer release on GitHub.',`<button class="pbtn" data-act="upd">Check now</button>`)}${updMsg?`<div class="sub">${esc(updMsg)}</div>`:''}
  ${row('Update automatically','Checks every few hours, downloads in the background, and installs when you restart. It never interrupts your work.',tg(s.auto_update,'auto_update'))}
  ${row('Release channel','Beta gets new features first.',`<div class="seg">${['stable','beta'].map(c=>`<button data-ch="${c}" class="${s.channel===c?'on':''}">${c[0].toUpperCase()+c.slice(1)}</button>`).join('')}</div>`)}`;
 if(setSec==='about')h=`<h2>About Wisp</h2><div class="about"><img src="/logo.svg" alt=""><div><b>Wisp ${esc(S.version)}</b><small>A light, glassy browser built on WebView2.</small></div></div>${row('Source code','github.com/Wisp-Browser/Wisp','<button class="pbtn" data-open="https://github.com/Wisp-Browser/Wisp">Open</button>')}`;
 const body=$('#setbody');if(nav!==setNavHtml){setNavHtml=nav;$('#setnav').innerHTML=nav}
 if(h!==setHtml){const top=setSecShown===setSec?body.scrollTop:0;setHtml=h;body.innerHTML=h;body.scrollTop=top;setSecShown=setSec}
 const b=$('#setm');wireLook(b);b.querySelectorAll('[data-sec]').forEach(x=>x.onclick=()=>{setSec=x.dataset.sec;capturing=null;if(setSec==='bookmarks')act('sources');renderSettings()});
 $('#setx').onclick=closeSettings;
 b.querySelectorAll('[data-set]').forEach(x=>x.onclick=()=>{act('set',{k:x.dataset.set,v:!s[x.dataset.set]})});
 const bt=$('#bmbartg');if(bt)bt.onclick=()=>act('set',{k:'bmbar',v:!S.bmbar});
 b.querySelectorAll('[data-eng]').forEach(x=>x.onclick=()=>act('engine',{id:x.dataset.eng}));
 b.querySelectorAll('[data-hib]').forEach(x=>x.onclick=()=>act('set',{k:'hibernate_s',v:+x.dataset.hib}));
 b.querySelectorAll('[data-ch]').forEach(x=>x.onclick=()=>act('set',{k:'channel',v:x.dataset.ch}));
 b.querySelectorAll('[data-un]').forEach(x=>x.onclick=()=>act('set',{k:'unblock_site',v:x.dataset.un}));
 b.querySelectorAll('[data-imp]').forEach(x=>x.onclick=()=>act('import',{src:x.dataset.imp}));
 const pt=$('#pwtg');if(pt)pt.onclick=()=>act('set',{k:'pw_save',v:!(S.pw&&S.pw.save)});
 b.querySelectorAll('[data-pw]').forEach(x=>x.onclick=()=>{if(x.dataset.pw==='manage')closeSettings();act(x.dataset.pw==='manage'?'pw_manage':'passkeys')});
 b.querySelectorAll('[data-pwi]').forEach(x=>x.onclick=()=>act('pw_import',{src:x.dataset.pwi}));
 b.querySelectorAll('[data-xs]').forEach(x=>x.onclick=()=>{closeSettings();act(x.dataset.xs==='store'?'ext_store':'ext_manage',{q:''})});
 b.querySelectorAll('[data-xt]').forEach(x=>x.onclick=()=>{const e=(S.exts||[]).find(y=>y.id===x.dataset.xt);act('ext_toggle',{id:x.dataset.xt,on:!(e&&e.enabled)})});
 b.querySelectorAll('[data-xo]').forEach(x=>x.onclick=()=>{closeSettings();act('ext_options',{id:x.dataset.xo})});
 b.querySelectorAll('[data-open]').forEach(x=>x.onclick=()=>{closeSettings();act('open',{url:x.dataset.open})});
 b.querySelectorAll('[data-key]').forEach(x=>x.onclick=()=>{capturing=x.dataset.key;renderSettings()});
 b.querySelectorAll('[data-c]').forEach(x=>x.onclick=()=>CMDS.find(c=>c.id===x.dataset.c).run());
 b.querySelectorAll('[data-act]').forEach(x=>x.onclick=()=>{const a=x.dataset.act;
  if(a==='theme')act('theme');else if(a==='filters'){toast('Downloading filter lists…');act('filters_update')}
  else if(a==='upd'){updMsg='Checking…';renderSettings();act('upd_check')}else if(a==='restart')act('update_apply')
  else if(a==='clear'){const w=[];if($('#c_h').checked)w.push('history');if($('#c_c').checked)w.push('cookies');if(w.length)act('clear',{what:w})}
  else if(a==='resetkeys'){S.s.shortcuts={};act('set',{k:'shortcuts',v:{}});pushCombos();renderSettings()}
  else if(a==='openbm'){closeSettings();openLib('bookmarks')}
  else if(a==='newtab'){closeSettings();act('new')}})}

// ---------- state + events ----------
function state(s){S=s;if(!booted)build();
 document.body.classList.toggle('private',!!s.private);$('#privbadge').classList.toggle('hidden',!s.private);
 const pg=s.page||{};$('#bmbtn').classList.toggle('on',!!pg.bm);const n=pg.blocked||0,sh=$('#shn');sh.textContent=n>99?'99+':n||'';$('#shbtn').classList.toggle('active',!!n);$('#shbtn').classList.toggle('off',!s.s.blocker||!!pg.site_off);
 const z=$('#zmchip');z.classList.toggle('hidden',!pg.zoom||pg.zoom===1);z.textContent=Math.round((pg.zoom||1)*100)+'%';
 renderPins();const xsg=JSON.stringify(s.exts||[]);if(popKind==='ext'&&xsg!==extSig){popKind=null;popover('ext')}extSig=xsg;
 pushCombos();if($('#setm').classList.contains('open')&&!capturing)renderSettings();
 if(s.setup&&!s.private&&!su.open&&!su.done)openSetup();else if(su.open)renderSetup()}
function event(e){switch(e.type){
 case'toast':toast(e.text);break;
 case'lib':if(e.what===lib.tab)renderLib(e.items);break;
 case'findcount':$('#fcount').textContent=$('#fq').value?(e.n?e.n+' found':'No matches'):'';break;
 case'sugg':if(e.n===sg.n){sg.items=e.items;renderSugg()}break;
 case'sugg_remote':if(e.n===sg.n){sg.remote=e.items;renderSugg()}break;
 case'sources':srcs=e.items;if(setSec==='bookmarks')renderSettings();break;
 case'upd':updMsg=e.msg;renderSettings();if(!$('#setm').classList.contains('open'))toast(e.msg);break;
 case'update_ready':updMsg='Version '+e.version+' is ready.';stickyToast('Update '+e.version+' ready',"Restart",()=>act('update_apply'));renderSettings();break;
 case'pageclick':closePop();if($('#extm').classList.contains('open'))extAnswer(false);closeSugg();if($('#palm').classList.contains('open'))palette(false);if($('#libm').classList.contains('open'))closeLib();if($('#setm').classList.contains('open')&&!capturing)closeSettings();if($('#spmodal')&&$('#spmodal').classList.contains('open'))$('#spmodal').classList.remove('open');$('#ctx').classList.remove('open');$('#spm').classList.remove('open');break;
 case'pw_offer':pwOffer(e);break;
 case'ext_confirm':closePop();extConfirm(e);break;
 case'ext_popup':extPopup=e.id||null;pinSig='';renderPins();break;
 case'imported':if($('#libm').classList.contains('open'))reqLib();break}}
window.M5={focusUrl,inset,state,event,relayout,openSettings,lib:openLib,key:c=>{if(c==='escape'&&S&&S.fullscreen){act('fullscreen');return}runCombo(c)},palette:()=>palette(true)};
})();
