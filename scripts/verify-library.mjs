// Verify real exported form controls in installed Edge without npm dependencies.
// Build first, then: node scripts/verify-library.mjs [path-to-ingenaro.exe]
import {spawn} from 'node:child_process';
import {readFile, writeFile, mkdir, mkdtemp} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';

const root=path.resolve(import.meta.dirname,'..');
const binary=path.resolve(process.argv[2]||path.join(root,'target/debug/ingenaro.exe'));
const edge='C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe';
const target=path.join(root,'target/library-browser-qa');
await mkdir(target,{recursive:true});
const directory=await mkdtemp(path.join(target,'run-'));
const gallery=JSON.parse(await readFile(path.join(root,'examples/library.ingenaro'),'utf8'));
const template=gallery.pages[0].widgets[0];
function widget(id,kind,text,name=kind){return {...structuredClone(template),id,kind,name,text,
    x:20,y:id*5,width:260,height:kind==='TextArea'||kind==='ListBox'?120:40,
    font_size:16,text_layout:null,radio_group:null,groups:[],vector_points:null,
    fill:'#FFFFFF',stroke:'#DDE2EB',hidden:false,locked:false,
    interaction:{action:'None',target_page:null,target_widget:null,value:''},image_data:null};}
const multiline='\nfirst line\n<second> & "quoted"\n</textarea><script>window.unsafeLibraryContent=true</script>';
const options='first\nsecond & "quoted"\n<last>';
const widgets=[widget(2,'Radio','One','choices'),widget(3,'Radio','Two','choices'),
    widget(4,'Input','initial'),widget(5,'TextArea',multiline),
    widget(6,'Droplist',options),widget(7,'ListBox',options)];
for(const [id,target,value]of [[10,4,'new <&" value'],[11,5,'new\nmultiline <&" value'],
    [12,6,'second & "quoted"'],[13,7,'<last>']]){
    const button=widget(id,'Button','Set '+target);
    button.interaction={action:'SetText',target_page:null,target_widget:target,value};
    widgets.push(button);
}
const database=structuredClone(gallery.pages.flatMap(page=>page.widgets).find(widget=>widget.name.startsWith('数据库 ')&&widget.vector_points));
Object.assign(database,{id:14,name:'QA database',x:400,y:20,fill:'#A855F7',stroke:'#FFFFFF00',opacity:1,groups:[]});
widgets.push(database);
const reversed=widget(15,'Rectangle','','QA reversed contours');
Object.assign(reversed,{x:400,y:160,width:160,height:100,fill:'#A855F7',stroke:'#FFFFFF00',vector_points:[
    [[0,0],[1,0],[1,1],[0,1]],[[.25,.25],[.25,.75],[.75,.75],[.75,.25]]
].map(points=>points.map(([X,Y])=>({Path:{X,Y},HigherSegmentExists:true})))});
widgets.push(reversed);
const page=(id,widgets)=>({id,name:'QA '+id,width:1000,height:900,canvas:'Fixed',background:'#FFFFFF',widgets});
const project={version:1,name:'Independent library controls QA',pages:[page(1,widgets),
    page(100,[widget(101,'Radio','Other page','choices')])],page_tree:[],interaction_events:[],next_id:1000};
const projectPath=path.join(directory,'controls.ingenaro');
await writeFile(projectPath,JSON.stringify(project));
const server=spawn(binary,['--preview-server',projectPath],{cwd:root,windowsHide:true,stdio:['pipe','pipe','pipe']});
let errors='';server.stderr.on('data',data=>errors+=data);
try{
    const url=await new Promise((resolve,reject)=>{
        let output='';const timer=setTimeout(()=>reject(new Error('Preview server timed out: '+errors)),20000);
        server.once('error',error=>{clearTimeout(timer);reject(error)});
        server.stdout.on('data',data=>{output+=data;const match=output.match(/http:\/\/127\.0\.0\.1:[^\s]+/);if(match){clearTimeout(timer);resolve(match[0])}});
        server.once('exit',code=>{clearTimeout(timer);reject(new Error('Preview server exited '+code+': '+errors))});
    });
    let html=await (await fetch(url)).text();
    const checks=async function(){
        const checks=[];
        function check(name,condition){checks.push({name,passed:!!condition});}
        const input=id=>document.querySelector('#w-'+id+' input');
        const control=id=>document.querySelector('#w-'+id+' textarea,#w-'+id+' select');
        check('textarea preserves leading newline and escaped multiline text',control(5).value===MULTILINE);
        check('escaped document content does not execute',!window.unsafeLibraryContent);
        input(2).click();input(3).click();
        check('same-page radio buttons are mutually exclusive',!input(2).checked&&input(3).checked);
        document.querySelector('#next-page').click();input(101).click();
        check('radio groups are isolated between pages',input(3).checked&&input(101).checked&&!document.querySelector('#page-100').hidden);
        document.querySelector('#previous-page').click();
        input(4).value='typed <&" value';input(4).dispatchEvent(new Event('input',{bubbles:true}));
        check('text field accepts editable values',input(4).value==='typed <&" value');
        control(5).value='typed\nmultiple lines';control(5).dispatchEvent(new Event('input',{bubbles:true}));
        check('textarea accepts multiple lines',control(5).value==='typed\nmultiple lines');
        control(6).value='second & "quoted"';control(6).dispatchEvent(new Event('change',{bubbles:true}));
        control(7).value='<last>';control(7).dispatchEvent(new Event('change',{bubbles:true}));
        check('dropdown options preserve escaped values',control(6).value==='second & "quoted"'&&control(6).options.length===3);
        check('listbox permits selection',control(7).value==='<last>'&&control(7).size===5);
        for(const id of [10,11,12,13])document.querySelector('#w-'+id).click();
        check('SetText changes text input',input(4).value==='new <&" value');
        check('SetText changes multiline input',control(5).value==='new\nmultiline <&" value');
        check('SetText selects dropdown and listbox values',control(6).value==='second & "quoted"'&&control(7).value==='<last>');
        async function purplePixel(id,x,y){
            try{
                const svg=document.querySelector('#w-'+id+' svg.vector-shape');
                const image=new Image();image.src='data:image/svg+xml;base64,'+btoa(new XMLSerializer().serializeToString(svg));
                await image.decode();
                const canvas=document.createElement('canvas');canvas.width=160;canvas.height=100;
                const ctx=canvas.getContext('2d');ctx.drawImage(image,0,0,160,100);
                const pixel=ctx.getImageData(x,y,1,1).data;
                return pixel[0]===168&&pixel[1]===85&&pixel[2]===247&&pixel[3]===255;
            }catch(error){return false;}
        }
        check('database overlapping contours retain their fill pixels',await purplePixel(14,80,13));
        check('opposite-direction overlapping contours remain filled',await purplePixel(15,80,50));
        const result={browser:navigator.userAgent,checks,passed:checks.every(check=>check.passed)};
        const output=document.createElement('pre');output.id='library-qa-result';output.textContent=JSON.stringify(result);document.body.append(output);
    }.toString().replace('MULTILINE',JSON.stringify(multiline).replaceAll('<','\\u003c'));
    html=html.replace('</body>','<script>('+checks+')()</script></body>');
    const htmlPath=path.join(directory,'controls.html');await writeFile(htmlPath,html);
    const dom=await new Promise((resolve,reject)=>{
        const browser=spawn(edge,['--headless=new','--disable-gpu','--no-first-run',
            '--user-data-dir='+path.join(directory,'edge-profile'),'--dump-dom',
            '--virtual-time-budget=2000',pathToFileURL(htmlPath).href],{windowsHide:true});
        let output='',stderr='';const timer=setTimeout(()=>{browser.kill();reject(new Error('Edge timed out'))},20000);
        browser.stdout.on('data',data=>output+=data);browser.stderr.on('data',data=>stderr+=data);
        browser.once('error',error=>{clearTimeout(timer);reject(error)});
        browser.once('exit',code=>{clearTimeout(timer);code===0?resolve(output):reject(new Error('Edge exited '+code+': '+stderr))});
    });
    await writeFile(path.join(directory,'dom.html'),dom);
    const encoded=dom.match(/<pre id="library-qa-result">([\s\S]*?)<\/pre>/)?.[1];
    if(!encoded)throw new Error('Browser did not produce QA results; inspect '+directory);
    const decode=value=>value.replaceAll('&lt;','<').replaceAll('&gt;','>').replaceAll('&amp;','&');
    const result=JSON.parse(decode(encoded));
    result.date=new Date().toISOString();
    result.scope='Independent exported HTML controls and vector fill pixels in installed Edge headless; local fixture only. No reference editor execution or full native/editor parity claim.';
    await writeFile(path.join(root,'docs/library-browser-qa.json'),JSON.stringify(result,null,2)+'\n');
    console.log(JSON.stringify(result,null,2));
    if(!result.passed)process.exitCode=1;
}finally{server.stdin.end('\n');server.kill();}
