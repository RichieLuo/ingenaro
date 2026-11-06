(()=>{
  'use strict';
  const $=selector=>document.querySelector(selector);
  const $$=selector=>[...document.querySelectorAll(selector)];
  const destinations={
    songyang:{name:'松阳 · 陈家铺',title:'去山里，住进一场清晨。',category:'山野之间 · 2—3 天',description:'沿着云雾里的石阶慢慢走，在百年村落喝一杯刚沏好的茶。这一次，给自己一个不赶路的周末。',days:[['入山，住进云雾里','午后抵达陈家铺，在村口吃一碗索面，沿着石阶走到山腰的民宿。傍晚把时间留给落日。'],['在茶香里醒来','清晨去茶山散步，坐进悬崖边的书店。午后找一家茶室，读几页书，或者什么也不做。'],['带一点山风回去','去老街吃一顿早饭，买一小袋当地的茶。慢慢告别，下次再来。']]},
    aranya:{name:'阿那亚 · 北戴河',title:'把时间读慢，把心事交给海。',category:'朝向大海 · 2 天',description:'一段留给海风和书页的假期。没有打卡清单，只有潮汐替你安排时间。',days:[['沿着海岸走走','午后到达，在海边散步，去小镇吃一顿晚饭。晚上听一场演出，或听海。'],['在日出之前醒来','带上外套和热茶看日出。找一处安静的座位，把上次没读完的书读完。']]},
    jingdezhen:{name:'景德镇 · 陶溪川',title:'捏一个晴天，带回日常。',category:'小城漫游 · 3 天',description:'让泥土和双手替生活松松绑。在老厂房、窑火与巷子之间，找回做一件小事的快乐。',days:[['沿着旧厂房漫游','在陶溪川的老建筑间走走，找一家小店喝咖啡。傍晚逛市集，遇见喜欢的杯子。'],['用双手慢下来','预约一堂陶艺体验课，学着拉坯。即使不够圆，也有属于自己的形状。'],['把小城装进口袋','早起去街巷吃碱水粑，寄一张明信片。留一件没做的事，给下一次。']]}
  };
  let currentFilter='all',currentDetail='songyang',toastTimer;
  const saved=new Set();
  const trips=[{id:1,name:'去松阳，过一个没有闹钟的周末',destination:'songyang',date:'',note:'想在山里喝一杯茶，把周末还给自己。',complete:false}];
  let nextTripId=2;
  function node(tag,className,text){const result=document.createElement(tag);if(className)result.className=className;if(text!==undefined)result.textContent=text;return result}
  function toast(text){clearTimeout(toastTimer);$('#toast').textContent=text;$('#toast').classList.add('show');toastTimer=setTimeout(()=>$('#toast').classList.remove('show'),3400)}
  function activateView(name){if(!['discover','trips','journal'].includes(name))name='discover';$$('.view').forEach(view=>view.hidden=view.id!=='view-'+name);$$('[data-nav]').forEach(link=>{if(link.dataset.nav===name)link.setAttribute('aria-current','page');else link.removeAttribute('aria-current')});$('#breadcrumb').textContent={discover:'发现路线',trips:'我的行程',journal:'旅行手记'}[name];if(name==='trips')renderTrips();window.scrollTo({top:0,behavior:'instant'})}
  function navigate(name){try{window.location.hash=name}catch{activateView(name)}}
  window.addEventListener('hashchange',()=>activateView(window.location.hash.slice(1)));
  $$('[data-nav]').forEach(link=>link.addEventListener('click',event=>{event.preventDefault();const name=link.dataset.nav;navigate(name);activateView(name)}));
  $$('a[href="#discover"]').filter(link=>!link.dataset.nav).forEach(link=>link.addEventListener('click',event=>{event.preventDefault();navigate('discover');activateView('discover')}));
  function renderRoutes(){const query=$('#route-search').value.trim().toLocaleLowerCase();let visible=0;$$('.route-card').forEach(card=>{const show=(currentFilter==='all'||currentFilter===card.dataset.category)&&(!$('#saved-only').checked||saved.has(card.dataset.id))&&card.dataset.search.toLocaleLowerCase().includes(query);card.hidden=!show;if(show)visible++});$('#routes-empty').hidden=visible!==0;$('#filter-status').textContent=visible?'已显示 '+visible+' 条路线':'没有匹配的路线'}
  $$('[data-filter]').forEach(button=>button.addEventListener('click',()=>{currentFilter=button.dataset.filter;$$('[data-filter]').forEach(item=>item.setAttribute('aria-pressed',String(item===button)));renderRoutes()}));
  $('#route-search').addEventListener('input',renderRoutes);
  $('#saved-only').addEventListener('change',renderRoutes);
  $('#reset-filters').addEventListener('click',()=>{$('#route-search').value='';$('#saved-only').checked=false;$('[data-filter="all"]').click();$('#route-search').focus()});
  $$('[data-save]').forEach(button=>button.addEventListener('click',()=>{const id=button.dataset.save;const isSaved=!saved.has(id);if(isSaved)saved.add(id);else saved.delete(id);button.setAttribute('aria-pressed',String(isSaved));button.setAttribute('aria-label',(isSaved?'取消收藏':'收藏')+destinations[id].name+'路线');renderRoutes();toast(isSaved?'已放进你的旅行口袋 · 当前预览有效':'已取消收藏，随时可以再遇见')}));
  function openDetail(id){currentDetail=id;const destination=destinations[id];$('#detail-title').textContent=destination.title;$('#detail-category').textContent=destination.category;$('#detail-description').textContent=destination.description;$('#detail-days').replaceChildren(...destination.days.map(([title,description])=>{const item=node('li');item.append(node('h3','',title),node('p','',description));return item}));$('#detail-dialog').showModal()}
  $('#feature-detail').addEventListener('click',()=>openDetail('songyang'));
  $$('[data-detail]').forEach(button=>button.addEventListener('click',()=>openDetail(button.dataset.detail)));
  function dateLiteral(date){const year=date.getFullYear(),month=String(date.getMonth()+1).padStart(2,'0'),day=String(date.getDate()).padStart(2,'0');return year+'-'+month+'-'+day}
  const today=new Date();$('#trip-date').min=dateLiteral(today);
  function openPlan(destination){$('#plan-form').reset();if(destination)$('#trip-destination').value=destination;const next=new Date();next.setDate(next.getDate()+7);$('#trip-date').value=dateLiteral(next);if(destination)$('#trip-name').value='去'+destinations[destination].name.split(' · ')[0]+'，慢慢走';$('#plan-dialog').showModal();$('#trip-name').focus()}
  $('#plan-top').addEventListener('click',()=>openPlan());$('#plan-trips').addEventListener('click',()=>openPlan());
  $('#detail-plan').addEventListener('click',()=>{$('#detail-dialog').close();openPlan(currentDetail)});
  // Script-only sandboxes intentionally disallow native form submission. Keep
  // accessible validation and Enter behavior, but dispatch our local controller
  // directly instead of requestSubmit/navigation or asking for allow-forms.
  function submitLocal(form){if(form.reportValidity())form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}))}
  $$('[data-submit]').forEach(button=>button.addEventListener('click',()=>submitLocal($('#'+button.dataset.submit))));
  $$('form').forEach(form=>form.addEventListener('keydown',event=>{if(event.key==='Enter'&&!event.isComposing&&!event.target.matches('textarea,button')){event.preventDefault();submitLocal(form)}}));
  $$('[data-close]').forEach(button=>button.addEventListener('click',()=>$('#'+button.dataset.close).close()));
  $$('dialog').forEach(dialog=>dialog.addEventListener('click',event=>{if(event.target!==dialog)return;const rect=dialog.getBoundingClientRect();if(event.clientX<rect.left||event.clientX>rect.right||event.clientY<rect.top||event.clientY>rect.bottom)dialog.close()}));
  function renderTrips(){const list=$('#trip-list');list.replaceChildren();if(!trips.length){const empty=node('div','empty-state');empty.append(node('h3','','留一页空白，给下一次出发。'),node('p','','在发现路线里找找灵感，或创建自己的行程。'));const create=node('button','button primary','创建行程');create.type='button';create.addEventListener('click',()=>openPlan());empty.append(create);list.append(empty);return}trips.forEach(trip=>{const card=node('article','trip-item');const date=trip.date?new Date(trip.date+'T12:00:00').toLocaleDateString('zh-CN',{year:'numeric',month:'long',day:'numeric'}):'日期待定 · 好天气就出发';card.append(node('span','trip-date',trip.complete?'A CHAPTER TO REMEMBER':date),node('h2','',trip.name),node('span','trip-place',destinations[trip.destination].name),node('p','',trip.note||'留白也是一种安排，沿路再遇见惊喜。'));const footer=node('div','trip-item-footer');footer.append(node('span','',trip.complete?'已放进旅行回忆':'计划中 · 不必赶路'));const toggle=node('button','text-button',trip.complete?'再次期待':'标记为已出发');toggle.type='button';toggle.addEventListener('click',()=>{trip.complete=!trip.complete;renderTrips();toast(trip.complete?'这段旅程，已经成为故事。':'又有一件值得期待的事了。')});const remove=node('button','text-button','移除');remove.type='button';remove.setAttribute('aria-label','移除行程：'+trip.name);remove.addEventListener('click',()=>{trips.splice(trips.indexOf(trip),1);renderTrips();toast('行程已移除 · 留点空间给下一次期待')});footer.append(toggle,remove);card.append(footer);list.append(card)})}
  $('#plan-form').addEventListener('submit',event=>{event.preventDefault();const name=$('#trip-name').value.trim();if(!name){$('#trip-name').setCustomValidity('给这段旅程起一个名字吧。');$('#trip-name').reportValidity();return}if(trips.length>=20){toast('当前预览最多保留 20 段行程，请先移除不需要的行程。');return}trips.unshift({id:nextTripId++,name,destination:$('#trip-destination').value,date:$('#trip-date').value,note:$('#trip-note').value.trim(),complete:false});$('#plan-dialog').close();navigate('trips');activateView('trips');$('#trips-title').setAttribute('tabindex','-1');$('#trips-title').focus({preventScroll:true});toast('行程已收好。接下来，等一个出发的日子。')});
  $('#trip-name').addEventListener('input',()=>$('#trip-name').setCustomValidity(''));
  $('#journal-entry').addEventListener('input',()=>{$('#journal-count').textContent=$('#journal-entry').value.length+' / 600';$('#journal-entry').setCustomValidity('')});
  $('#journal-form').addEventListener('submit',event=>{event.preventDefault();const text=$('#journal-entry').value.trim();if(!text){$('#journal-entry').setCustomValidity('写下一点真实的心情吧。');$('#journal-entry').reportValidity();return}const notes=$('#journal-notes');if(notes.children.length>=12){toast('这一页已经很充实了，当前预览最多留下 12 则手记。');return}notes.prepend(node('p','personal-note',text));$('#journal-entry').value='';$('#journal-count').textContent='0 / 600';toast('这一刻，替你记住了。仅保留在当前预览。')});
  const theme=$('#theme-toggle');let dark=matchMedia('(prefers-color-scheme: dark)').matches;
  function applyTheme(){document.documentElement.dataset.theme=dark?'dark':'light';theme.setAttribute('aria-label',dark?'切换浅色主题':'切换深色主题');theme.setAttribute('aria-pressed',String(dark));theme.querySelector('use').setAttribute('href',dark?'#i-sun':'#i-moon')}
  theme.addEventListener('click',()=>{dark=!dark;applyTheme()});applyTheme();
  document.addEventListener('keydown',event=>{const editing=event.target.matches('input,textarea,select,[contenteditable]');if(event.key==='/'&&!editing&&!$('dialog[open]')){event.preventDefault();navigate('discover');activateView('discover');$('#route-search').focus()}if(event.key==='Escape')$('#toast').classList.remove('show')});
  activateView(window.location.hash.slice(1));renderRoutes();renderTrips();
})();
