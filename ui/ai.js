// Wisp AI sidebar UI. Talks to Rust via window.ipc; Rust streams back through AI.event().
(function(){
const $=s=>document.querySelector(s),post=o=>window.ipc.postMessage(JSON.stringify(o));
const PROVS=[
 {id:'gemini',name:'Google Gemini',key:true,link:'https://aistudio.google.com/apikey',def:'gemini-2.5-flash'},
 {id:'groq',name:'Groq',key:true,link:'https://console.groq.com/keys',def:'llama-3.3-70b-versatile'},
 {id:'openrouter',name:'OpenRouter (free models)',key:true,link:'https://openrouter.ai/keys',def:'meta-llama/llama-3.3-70b-instruct:free'},
 {id:'ollama',name:'Ollama (local)',def:''},
 {id:'web:claude',name:'Claude (web login)',url:'https://claude.ai'},
 {id:'web:chatgpt',name:'ChatGPT (web login)',url:'https://chatgpt.com'},
 {id:'web:gemini',name:'Gemini (web login)',url:'https://gemini.google.com'}];
const LANGS=['English','Spanish','French','German','Portuguese','Italian','Japanese','Korean','Chinese','Arabic','Hindi','Russian'];
const CTXS=[['none','No page context'],['page','This page'],['tabs','All open tabs']];
let A={open:false,w:380,keys:{},cfg:{provider:'ollama',models:{},ollama:'http://127.0.0.1:11434',lang:'English',ctx:'none'},
  chats:[],cur:null,streaming:null,models:{},hist:false,space:null,first:true,keyerr:'',draft:''};
const esc=s=>String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
const prov=()=>PROVS.find(p=>p.id===A.cfg.provider)||PROVS[3];
const isWeb=()=>!!prov().url;
const chat=()=>A.chats.find(c=>c.id===A.cur);
const model=()=>{const p=prov();return (A.cfg.models&&A.cfg.models[p.id])||((A.models[p.id]&&A.models[p.id].list||[])[0])||p.def||''};
let saveT=0,cfgT=0;
function saveChats(){clearTimeout(saveT);saveT=setTimeout(()=>{if(A.space==null)return;
  const keep=A.chats.slice(0,60).map(c=>({...c,messages:c.messages.slice(-60)}));post({t:'ai_save',space:A.space,chats:keep})},500)}
function saveCfg(){clearTimeout(cfgT);cfgT=setTimeout(()=>post({t:'ai_cfg',cfg:A.cfg}),300)}

// ---------- markdown ----------
function inl(t){const codes=[];t=t.replace(/`([^`\n]+)`/g,(m,c)=>{codes.push(c);return '\u0000'+(codes.length-1)+'\u0000'});
  t=t.replace(/\*\*([^*\n]+)\*\*/g,'<b>$1</b>').replace(/(^|[^*\w])\*([^*\n]+)\*(?!\*)/g,'$1<i>$2</i>').replace(/~~([^~\n]+)~~/g,'<s>$1</s>')
   .replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,'<a href="$2" data-l="1">$1</a>');
  return t.replace(/\u0000(\d+)\u0000/g,(m,i)=>'<code>'+codes[i]+'</code>')}
function block(t){const L=t.split('\n'),out=[];let i=0;
 while(i<L.length){const l=L[i];
  if(/^\s*$/.test(l)){i++;continue}
  let m=l.match(/^(#{1,4})\s+(.*)$/);if(m){out.push(`<h${m[1].length}>${inl(m[2])}</h${m[1].length}>`);i++;continue}
  if(/^\s*([-*_])\1\1+\s*$/.test(l)){out.push('<hr>');i++;continue}
  if(/^\s*&gt;/.test(l)){const q=[];while(i<L.length&&/^\s*&gt;/.test(L[i])){q.push(L[i].replace(/^\s*&gt;\s?/,''));i++}out.push('<blockquote>'+block(q.join('\n'))+'</blockquote>');continue}
  if(/^\s*[-*•]\s+/.test(l)){const it=[];while(i<L.length&&/^\s*[-*•]\s+/.test(L[i])){it.push('<li>'+inl(L[i].replace(/^\s*[-*•]\s+/,''))+'</li>');i++}out.push('<ul>'+it.join('')+'</ul>');continue}
  if(/^\s*\d+[.)]\s+/.test(l)){const it=[];while(i<L.length&&/^\s*\d+[.)]\s+/.test(L[i])){it.push('<li>'+inl(L[i].replace(/^\s*\d+[.)]\s+/,''))+'</li>');i++}out.push('<ol>'+it.join('')+'</ol>');continue}
  if(/^\s*\|.*\|\s*$/.test(l)&&i+1<L.length&&/^\s*\|?[\s:|-]+\|[\s:|-]*$/.test(L[i+1])){const row=r=>r.trim().replace(/^\||\|$/g,'').split('|').map(c=>c.trim());
    const h=row(l);i+=2;const body=[];while(i<L.length&&/^\s*\|.*\|\s*$/.test(L[i])){body.push(row(L[i]));i++}
    out.push('<table><thead><tr>'+h.map(c=>'<th>'+inl(c)+'</th>').join('')+'</tr></thead><tbody>'+body.map(r=>'<tr>'+r.map(c=>'<td>'+inl(c)+'</td>').join('')+'</tr>').join('')+'</tbody></table>');continue}
  const p=[];while(i<L.length&&!/^\s*$/.test(L[i])&&!/^(#{1,4}\s|\s*[-*•]\s|\s*\d+[.)]\s|\s*&gt;)/.test(L[i])){p.push(L[i]);i++}
  if(p.length)out.push('<p>'+inl(p.join('<br>'))+'</p>');else i++}
 return out.join('')}
function md(src){return src.split(/(```[\s\S]*?(?:```|$))/g).map(p=>{
  if(p.startsWith('```')){const m=p.match(/^```([^\n`]*)\n?([\s\S]*?)(?:```)?$/),lang=(m[1]||'').trim(),code=(m[2]||'').replace(/\n$/,'');
   return `<div class="cb"><div class="ch"><span>${esc(lang||'code')}</span><button class="cp">Copy</button></div><pre><code>${esc(code)}</code></pre></div>`}
  return block(esc(p))}).join('')}

// ---------- dropdowns ----------
function closeMenus(){document.querySelectorAll('.ddm.open').forEach(m=>m.classList.remove('open'));if(isWeb())post({t:'ai_webhide',hide:false})}
function dd(host,items,value,pick,empty){host.innerHTML='';const b=document.createElement('button');b.className='ddb';b.innerHTML='<span></span><i>▾</i>';
 const cur=items.find(i=>i.id===value);b.firstChild.textContent=cur?cur.name:(value||empty||'Select…');
 const m=document.createElement('div');m.className='ddm panel';
 items.forEach(i=>{const o=document.createElement('button');o.textContent=i.name;o.title=i.name;if(i.id===value)o.className='on';o.onclick=e=>{e.stopPropagation();closeMenus();pick(i.id)};m.appendChild(o)});
 if(!items.length){const o=document.createElement('button');o.textContent=empty||'Nothing here yet';o.style.opacity='.6';m.appendChild(o)}
 b.onclick=e=>{e.stopPropagation();const was=m.classList.contains('open');closeMenus();if(!was){m.classList.add('open');if(isWeb())post({t:'ai_webhide',hide:true})}};host.append(b,m);return m}

// ---------- build ----------
function build(){const p=$('#aip');p.className='panel';
 p.innerHTML=`<div id="aiw"></div>
 <div class="ah"><div class="ar1"><span class="at"><img src="/logo.svg" alt="">Wisp AI</span><div class="ab">
  <button class="ib" id="aih" title="Chat history">⟲</button><button class="ib" id="ain" title="New chat">＋</button><button class="ib" id="aix" title="Close (Ctrl+J)"><svg viewBox="0 0 10 10" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"><path d="M2 2l6 6M8 2l-6 6"/></svg></button></div></div>
  <div class="ar2"><div class="dd" id="aprov"></div><div class="dd" id="amod"></div></div></div>
 <div class="abody"><div id="amsgs"></div><div class="ahist" id="ahist"></div></div>
 <div class="afoot"><div class="aqs"><button class="q" id="qsum">Summarize page</button><button class="q" id="qask">Ask about page</button><button class="q" id="qtr">Translate ▾</button><button class="q" id="qtabs">All tabs</button></div>
  <div class="comp"><textarea id="ain2" rows="1" placeholder="Ask anything…" spellcheck="true"></textarea><button class="send" id="asend" title="Send">➤</button></div></div>`;
 $('#aix').onclick=()=>post({t:'ai_panel',open:false});$('#ain').onclick=newChat;$('#aih').onclick=()=>{A.hist=!A.hist;renderHist()};
 $('#asend').onclick=()=>A.streaming?stop():submit();
 const ta=$('#ain2');ta.oninput=()=>{ta.style.height='auto';ta.style.height=Math.min(ta.scrollHeight,130)+'px';A.draft=ta.value};
 ta.onkeydown=e=>{if(e.key==='Enter'&&!e.shiftKey){e.preventDefault();submit()}else if(e.key==='Escape'&&A.streaming)stop()};
 $('#qsum').onclick=()=>send('Summarize this page. Start with a one-sentence TL;DR, then key points.','page');
 $('#qask').onclick=()=>{A.cfg.ctx=A.cfg.ctx==='page'?'none':'page';saveCfg();renderFoot();ta.focus()};
 $('#qtabs').onclick=()=>{A.cfg.ctx=A.cfg.ctx==='tabs'?'none':'tabs';saveCfg();renderFoot();ta.focus()};
 $('#qtr').onclick=e=>{e.stopPropagation();const m=document.createElement('div');m.className='ddm panel open';m.style.cssText='bottom:32px;top:auto;left:0;min-width:150px;position:absolute';
  LANGS.forEach(l=>{const o=document.createElement('button');o.textContent=l;if(l===A.cfg.lang)o.className='on';o.onclick=ev=>{ev.stopPropagation();A.cfg.lang=l;saveCfg();m.remove();send(`Translate to ${l}. Preserve meaning and formatting.`,'sel_or_page')};m.appendChild(o)});
  const old=$('#qtr .ddm');if(old){old.remove();return}$('#qtr').appendChild(m)};
 const h=$('#aiw');let drag=false;h.onpointerdown=e=>{drag=true;h.setPointerCapture(e.pointerId);h.classList.add('live')};
 h.onpointermove=e=>{if(!drag)return;A.w=Math.max(300,Math.min(640,innerWidth-12-e.clientX));layout();post({t:'ai_w',w:A.w})};
 h.onpointerup=()=>{drag=false;h.classList.remove('live');post({t:'ai_wend'})};
 document.addEventListener('click',e=>{closeMenus();if(!e.target.closest('#qtr')){const m=$('#qtr .ddm');if(m)m.remove()}});
 $('#amsgs').addEventListener('click',e=>{
  const a=e.target.closest('a[data-l]');if(a){e.preventDefault();post({t:'open',url:a.getAttribute('href')});return}
  const c=e.target.closest('.cp');if(c){const code=c.closest('.cb').querySelector('code').textContent;try{navigator.clipboard.writeText(code)}catch(_){}c.textContent='Copied';setTimeout(()=>c.textContent='Copy',1200)}});
}
function layout(){const r=document.documentElement.style;r.setProperty('--aiw',A.w+'px');r.setProperty('--rt',A.open?(12+A.w+8)+'px':'12px');
 const p=$('#aip');p.classList.toggle('open',A.open);p.classList.toggle('web',isWeb());$('#aibtn').classList.toggle('on',A.open)}

// ---------- render ----------
function needsKey(){const p=prov();return p.key&&!A.keys[p.id]}
function renderHead(){
 dd($('#aprov'),PROVS.map(p=>({id:p.id,name:p.name})),A.cfg.provider,id=>{A.cfg.provider=id;saveCfg();onProvider()});
 const p=prov(),mh=$('#amod');mh.style.display=p.url?'none':'';
 if(!p.url){const ms=A.models[p.id];const list=((ms&&ms.list)||[]).map(x=>({id:x,name:x}));
  if(!list.length&&p.def)list.push({id:p.def,name:p.def});
  dd(mh,list,model(),id=>{A.cfg.models[p.id]=id;saveCfg();renderHead()},ms&&ms.loading?'Loading models…':(p.id==='ollama'?'No models found':'Model'))}}
function setupCard(){const p=prov(),c=document.createElement('div');c.className='empty';
 c.innerHTML=`<img src="/logo.svg" alt=""><h3>Connect ${esc(p.name.replace(/ \(.*/,''))}</h3><p>Free tier works. Paste an API key — it's stored in the Windows Credential Manager and is never shown again.</p>
  <div class="card"><div class="row"><input id="kin" type="password" placeholder="Paste API key" autocomplete="off" spellcheck="false"><button class="pb go" id="ksave">Save</button></div>
  <small id="kerr" style="color:#ff8f8f">${esc(A.keyerr)}</small><span class="lk" id="klink">Get a free key ↗</span></div>`;
 setTimeout(()=>{const i=c.querySelector('#kin'),s=()=>{const v=i.value.trim();if(v){A.keyerr='';post({t:'ai_key_set',provider:p.id,key:v});i.value=''}};
  c.querySelector('#ksave').onclick=s;i.onkeydown=e=>{if(e.key==='Enter')s()};c.querySelector('#klink').onclick=()=>post({t:'open',url:p.link})});return c}
function emptyCard(){const p=prov(),c=document.createElement('div');c.className='empty';
 let extra='';
 if(p.id==='ollama'){const ms=A.models.ollama;extra=`<div class="ollama"><label>Ollama address</label><div class="row"><input id="oin" value="${esc(A.cfg.ollama)}" spellcheck="false"><button class="pb" id="oretry">Connect</button></div>
   ${ms&&ms.error?`<p style="margin-top:8px;color:#ff8f8f">${esc(ms.error)}</p><p style="margin-top:6px">Install Ollama from <span class="lk" id="olink">ollama.com</span>, then run <code>ollama pull llama3.2</code>.</p>`:''}</div>`}
 else if(p.key)extra=`<p style="margin-top:12px"><span class="lk" id="krm">Remove saved key</span></p>`;
 c.innerHTML=`<img src="/logo.svg" alt=""><h3>How can I help?</h3><p>Chat, or use the shortcuts below to work with the page you're on.<br>Tip: select text on any page, right-click, and choose “Explain with Wisp AI”.</p>${extra}`;
 setTimeout(()=>{const r=c.querySelector('#oretry');if(r){r.onclick=()=>{A.cfg.ollama=c.querySelector('#oin').value.trim()||'http://127.0.0.1:11434';saveCfg();A.models.ollama={loading:true};renderHead();post({t:'ai_models',provider:'ollama'})};
   const l=c.querySelector('#olink');if(l)l.onclick=()=>post({t:'open',url:'https://ollama.com'})}
  const k=c.querySelector('#krm');if(k)k.onclick=()=>post({t:'ai_key_del',provider:p.id})});return c}
function msgEl(m,i,last){const d=document.createElement('div');d.className='m '+(m.role==='user'?'user':'ai')+(m.error?' err':'');
 if(m.role==='user'){d.innerHTML=md(m.content);return d}
 const live=A.streaming&&A.streaming.asst===m;
 d.innerHTML=`<div class="mb">${md(m.content)}${live?'<span class="cur"></span>':''}</div>${m.error?`<div class="mb" style="margin-top:6px">⚠ ${esc(m.error)}</div>`:''}${live||!m.content?'':'<div class="mact"><button class="mcp">Copy</button></div>'}`;
 const b=d.querySelector('.mcp');if(b)b.onclick=()=>{try{navigator.clipboard.writeText(m.content)}catch(_){}b.textContent='Copied';setTimeout(()=>b.textContent='Copy',1100)};return d}
function renderMsgs(keepScroll){const c=$('#amsgs');if(!c)return;const near=c.scrollHeight-c.scrollTop-c.clientHeight<80;c.innerHTML='';A.streamEl=null;
 if(needsKey()){c.appendChild(setupCard());return}
 const ch=chat();if(!ch||!ch.messages.length){c.appendChild(emptyCard());return}
 ch.messages.forEach((m,i)=>{const e=msgEl(m,i);c.appendChild(e);if(A.streaming&&A.streaming.asst===m)A.streamEl=e});
 if(!keepScroll||near)c.scrollTop=c.scrollHeight}
function renderHist(){const h=$('#ahist');h.classList.toggle('open',A.hist);if(!A.hist)return;h.innerHTML='';
 if(!A.chats.length){h.innerHTML='<div class="empty"><p>No saved chats in this space yet.</p></div>';return}
 [...A.chats].sort((a,b)=>b.ts-a.ts).forEach(c=>{const r=document.createElement('div');r.className='hi'+(c.id===A.cur?' on':'');
  r.innerHTML=`<span></span><small>${new Date(c.ts).toLocaleDateString([],{month:'short',day:'numeric'})}</small><button class="ib" title="Delete"><svg viewBox="0 0 10 10" width="9" height="9" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"><path d="M2 2l6 6M8 2l-6 6"/></svg></button>`;r.children[0].textContent=c.title||'New chat';
  r.onclick=()=>{A.cur=c.id;A.hist=false;renderHist();renderMsgs()};
  r.querySelector('button').onclick=e=>{e.stopPropagation();A.chats=A.chats.filter(x=>x.id!==c.id);if(A.cur===c.id)A.cur=null;saveChats();renderHist();renderMsgs()};h.appendChild(r)})}
function renderFoot(){const s=A.streaming;$('#asend').textContent=s?'■':'➤';$('#asend').title=s?'Stop':'Send';
 $('#qask').classList.toggle('on',A.cfg.ctx==='page');$('#qtabs').classList.toggle('on',A.cfg.ctx==='tabs');
 $('#ain2').placeholder=A.cfg.ctx==='page'?'Ask about this page…':A.cfg.ctx==='tabs'?'Ask about all open tabs…':'Ask anything…';
 $('#aip .afoot').style.opacity=needsKey()?.45:1}

// ---------- actions ----------
function newChat(){A.cur=null;A.hist=false;renderHist();renderMsgs();$('#ain2').focus()}
function ensureChat(){let c=chat();if(!c){c={id:'c'+Date.now(),title:'',ts:Date.now(),messages:[]};A.chats.unshift(c);A.cur=c.id}return c}
function submit(){const ta=$('#ain2'),t=ta.value;if(!t.trim())return;ta.value='';ta.style.height='auto';A.draft='';send(t)}
function send(text,ctx){text=text.trim();if(!text||A.streaming||isWeb()||needsKey())return;
 const c=ensureChat(),asst={role:'assistant',content:''};c.messages.push({role:'user',content:text},asst);c.ts=Date.now();if(!c.title)c.title=text.replace(/\s+/g,' ').slice(0,42);
 const id='r'+Date.now()+Math.random().toString(36).slice(2,6);A.streaming={id,chat:c,asst};A.hist=false;renderHist();renderMsgs();renderFoot();
 post({t:'ai_send',id,provider:A.cfg.provider,model:model(),ctx:ctx||A.cfg.ctx,messages:c.messages.slice(0,-1).map(m=>({role:m.role,content:m.content}))})}
function stop(){const s=A.streaming;if(!s)return;post({t:'ai_stop',id:s.id});A.streaming=null;renderMsgs(true);renderFoot();saveChats()}
function onProvider(){renderHead();layout();post({t:'ai_web',url:prov().url||null});if(!isWeb()){fetchModels();}renderMsgs();renderFoot()}
function fetchModels(){const p=prov();if(p.url||(p.key&&!A.keys[p.id]))return;A.models[p.id]={...(A.models[p.id]||{}),loading:true};post({t:'ai_models',provider:p.id});renderHead()}

// ---------- host callbacks ----------
function event(e){
 if(e.type==='delta'){const s=A.streaming;if(!s||s.id!==e.id)return;s.asst.content+=e.text;
  if(A.streamEl){const mb=A.streamEl.querySelector('.mb'),c=$('#amsgs'),near=c.scrollHeight-c.scrollTop-c.clientHeight<90;mb.innerHTML=md(s.asst.content)+'<span class="cur"></span>';if(near)c.scrollTop=c.scrollHeight}else renderMsgs(true)}
 else if(e.type==='done'){const s=A.streaming;if(!s||s.id!==e.id)return;if(e.error)s.asst.error=e.error;A.streaming=null;renderMsgs(true);renderFoot();saveChats()}
 else if(e.type==='models'){A.models[e.provider]={list:e.models,error:e.error};if(e.provider===A.cfg.provider){
   if(e.models.length&&!e.models.includes(A.cfg.models[e.provider])){const keep=A.cfg.models[e.provider];if(keep)delete A.cfg.models[e.provider]}
   renderHead();renderMsgs(true)}}
 else if(e.type==='keyerr'){A.keyerr=e.msg;renderMsgs()}
 else if(e.type==='chats'){if(e.space!==A.space)return;A.chats=Array.isArray(e.chats)?e.chats:[];A.cur=null;renderMsgs();renderHist()}
 else if(e.type==='selection'){if(isWeb()){A.cfg.provider=PROVS.find(p=>!p.url).id;onProvider()}
   const q=e.text.replace(/\n/g,'\n> ');A.cur=null;
   if(e.mode==='translate')send(`Translate to ${A.cfg.lang}:\n\n> ${q}`,'none');else send(`Explain this selected text clearly and briefly:\n\n> ${q}`,'none')}}

function state(S){const ai=S.ai||{};const wasOpen=A.open;A.open=!!ai.open;A.w=ai.w||380;A.keys=ai.keys||{};
 if(A.first){A.first=false;A.cfg=Object.assign(A.cfg,ai.cfg||{});A.cfg.models=A.cfg.models||{};build();post({t:'ai_load'});A.space=S.cur;onProvider()}
 else if(S.cur!==A.space){A.space=S.cur;A.cur=null;A.chats=[];post({t:'ai_load'});renderMsgs();renderHist()}
 layout();
 if(A.open&&!wasOpen){renderHead();if(!isWeb()&&!A.models[prov().id])fetchModels();setTimeout(()=>{const t=$('#ain2');if(t)t.focus()},250)}
 const needs=needsKey();if(!A.lastNeeds!==!needs||A.lastNeeds===undefined){A.lastNeeds=needs;renderMsgs(true);renderFoot();if(!needs&&A.keys[prov().id]&&!A.models[prov().id])fetchModels()}}
window.AI={state,event,toggle:()=>post({t:'ai_panel',open:!A.open})};
})();
