var De=Object.create;var Ht=Object.defineProperty;var Fe=Object.getOwnPropertyDescriptor;var ve=Object.getOwnPropertyNames;var ze=Object.getPrototypeOf,Ve=Object.prototype.hasOwnProperty;var y=(n,t)=>()=>(t||n((t={exports:{}}).exports,t),t.exports);var $e=(n,t,e,r)=>{if(t&&typeof t=="object"||typeof t=="function")for(let o of ve(t))!Ve.call(n,o)&&o!==e&&Ht(n,o,{get:()=>t[o],enumerable:!(r=Fe(t,o))||r.enumerable});return n};var Oe=(n,t,e)=>(e=n!=null?De(ze(n)):{},$e(t||!n||!n.__esModule?Ht(e,"default",{value:n,enumerable:!0}):e,n));var Ut=y((jn,kt)=>{kt.exports=function(){return typeof Promise=="function"&&Promise.prototype&&Promise.prototype.then}});var M=y(q=>{var ut,Ke=[0,26,44,70,100,134,172,196,242,292,346,404,466,532,581,655,733,815,901,991,1085,1156,1258,1364,1474,1588,1706,1828,1921,2051,2185,2323,2465,2611,2761,2876,3034,3196,3362,3532,3706];q.getSymbolSize=function(t){if(!t)throw new Error('"version" cannot be null or undefined');if(t<1||t>40)throw new Error('"version" should be in range from 1 to 40');return t*4+17};q.getSymbolTotalCodewords=function(t){return Ke[t]};q.getBCHDigit=function(n){let t=0;for(;n!==0;)t++,n>>>=1;return t};q.setToSJISFunction=function(t){if(typeof t!="function")throw new Error('"toSJISFunc" is not a valid function.');ut=t};q.isKanjiModeEnabled=function(){return typeof ut<"u"};q.toSJIS=function(t){return ut(t)}});var Z=y(x=>{x.L={bit:1};x.M={bit:0};x.Q={bit:3};x.H={bit:2};function Je(n){if(typeof n!="string")throw new Error("Param is not a string");switch(n.toLowerCase()){case"l":case"low":return x.L;case"m":case"medium":return x.M;case"q":case"quartile":return x.Q;case"h":case"high":return x.H;default:throw new Error("Unknown EC Level: "+n)}}x.isValid=function(t){return t&&typeof t.bit<"u"&&t.bit>=0&&t.bit<4};x.from=function(t,e){if(x.isValid(t))return t;try{return Je(t)}catch{return e}}});var Ft=y((Wn,Dt)=>{function _t(){this.buffer=[],this.length=0}_t.prototype={get:function(n){let t=Math.floor(n/8);return(this.buffer[t]>>>7-n%8&1)===1},put:function(n,t){for(let e=0;e<t;e++)this.putBit((n>>>t-e-1&1)===1)},getLengthInBits:function(){return this.length},putBit:function(n){let t=Math.floor(this.length/8);this.buffer.length<=t&&this.buffer.push(0),n&&(this.buffer[t]|=128>>>this.length%8),this.length++}};Dt.exports=_t});var zt=y((Zn,vt)=>{function $(n){if(!n||n<1)throw new Error("BitMatrix size must be defined and greater than 0");this.size=n,this.data=new Uint8Array(n*n),this.reservedBit=new Uint8Array(n*n)}$.prototype.set=function(n,t,e,r){let o=n*this.size+t;this.data[o]=e,r&&(this.reservedBit[o]=!0)};$.prototype.get=function(n,t){return this.data[n*this.size+t]};$.prototype.xor=function(n,t,e){this.data[n*this.size+t]^=e};$.prototype.isReserved=function(n,t){return this.reservedBit[n*this.size+t]};vt.exports=$});var Vt=y(X=>{var Ye=M().getSymbolSize;X.getRowColCoords=function(t){if(t===1)return[];let e=Math.floor(t/7)+2,r=Ye(t),o=r===145?26:Math.ceil((r-13)/(2*e-2))*2,i=[r-7];for(let s=1;s<e-1;s++)i[s]=i[s-1]-o;return i.push(6),i.reverse()};X.getPositions=function(t){let e=[],r=X.getRowColCoords(t),o=r.length;for(let i=0;i<o;i++)for(let s=0;s<o;s++)i===0&&s===0||i===0&&s===o-1||i===o-1&&s===0||e.push([r[i],r[s]]);return e}});var Kt=y(Ot=>{var je=M().getSymbolSize,$t=7;Ot.getPositions=function(t){let e=je(t);return[[0,0],[e-$t,0],[0,e-$t]]}});var Jt=y(w=>{w.Patterns={PATTERN000:0,PATTERN001:1,PATTERN010:2,PATTERN011:3,PATTERN100:4,PATTERN101:5,PATTERN110:6,PATTERN111:7};var R={N1:3,N2:3,N3:40,N4:10};w.isValid=function(t){return t!=null&&t!==""&&!isNaN(t)&&t>=0&&t<=7};w.from=function(t){return w.isValid(t)?parseInt(t,10):void 0};w.getPenaltyN1=function(t){let e=t.size,r=0,o=0,i=0,s=null,l=null;for(let c=0;c<e;c++){o=i=0,s=l=null;for(let f=0;f<e;f++){let a=t.get(c,f);a===s?o++:(o>=5&&(r+=R.N1+(o-5)),s=a,o=1),a=t.get(f,c),a===l?i++:(i>=5&&(r+=R.N1+(i-5)),l=a,i=1)}o>=5&&(r+=R.N1+(o-5)),i>=5&&(r+=R.N1+(i-5))}return r};w.getPenaltyN2=function(t){let e=t.size,r=0;for(let o=0;o<e-1;o++)for(let i=0;i<e-1;i++){let s=t.get(o,i)+t.get(o,i+1)+t.get(o+1,i)+t.get(o+1,i+1);(s===4||s===0)&&r++}return r*R.N2};w.getPenaltyN3=function(t){let e=t.size,r=0,o=0,i=0;for(let s=0;s<e;s++){o=i=0;for(let l=0;l<e;l++)o=o<<1&2047|t.get(s,l),l>=10&&(o===1488||o===93)&&r++,i=i<<1&2047|t.get(l,s),l>=10&&(i===1488||i===93)&&r++}return r*R.N3};w.getPenaltyN4=function(t){let e=0,r=t.data.length;for(let i=0;i<r;i++)e+=t.data[i];return Math.abs(Math.ceil(e*100/r/5)-10)*R.N4};function Ge(n,t,e){switch(n){case w.Patterns.PATTERN000:return(t+e)%2===0;case w.Patterns.PATTERN001:return t%2===0;case w.Patterns.PATTERN010:return e%3===0;case w.Patterns.PATTERN011:return(t+e)%3===0;case w.Patterns.PATTERN100:return(Math.floor(t/2)+Math.floor(e/3))%2===0;case w.Patterns.PATTERN101:return t*e%2+t*e%3===0;case w.Patterns.PATTERN110:return(t*e%2+t*e%3)%2===0;case w.Patterns.PATTERN111:return(t*e%3+(t+e)%2)%2===0;default:throw new Error("bad maskPattern:"+n)}}w.applyMask=function(t,e){let r=e.size;for(let o=0;o<r;o++)for(let i=0;i<r;i++)e.isReserved(i,o)||e.xor(i,o,Ge(t,i,o))};w.getBestMask=function(t,e){let r=Object.keys(w.Patterns).length,o=0,i=1/0;for(let s=0;s<r;s++){e(s),w.applyMask(s,t);let l=w.getPenaltyN1(t)+w.getPenaltyN2(t)+w.getPenaltyN3(t)+w.getPenaltyN4(t);w.applyMask(s,t),l<i&&(i=l,o=s)}return o}});var ft=y(dt=>{var B=Z(),tt=[1,1,1,1,1,1,1,1,1,1,2,2,1,2,2,4,1,2,4,4,2,4,4,4,2,4,6,5,2,4,6,6,2,5,8,8,4,5,8,8,4,5,8,11,4,8,10,11,4,9,12,16,4,9,16,16,6,10,12,18,6,10,17,16,6,11,16,19,6,13,18,21,7,14,21,25,8,16,20,25,8,17,23,25,9,17,23,34,9,18,25,30,10,20,27,32,12,21,29,35,12,23,34,37,12,25,34,40,13,26,35,42,14,28,38,45,15,29,40,48,16,31,43,51,17,33,45,54,18,35,48,57,19,37,51,60,19,38,53,63,20,40,56,66,21,43,59,70,22,45,62,74,24,47,65,77,25,49,68,81],et=[7,10,13,17,10,16,22,28,15,26,36,44,20,36,52,64,26,48,72,88,36,64,96,112,40,72,108,130,48,88,132,156,60,110,160,192,72,130,192,224,80,150,224,264,96,176,260,308,104,198,288,352,120,216,320,384,132,240,360,432,144,280,408,480,168,308,448,532,180,338,504,588,196,364,546,650,224,416,600,700,224,442,644,750,252,476,690,816,270,504,750,900,300,560,810,960,312,588,870,1050,336,644,952,1110,360,700,1020,1200,390,728,1050,1260,420,784,1140,1350,450,812,1200,1440,480,868,1290,1530,510,924,1350,1620,540,980,1440,1710,570,1036,1530,1800,570,1064,1590,1890,600,1120,1680,1980,630,1204,1770,2100,660,1260,1860,2220,720,1316,1950,2310,750,1372,2040,2430];dt.getBlocksCount=function(t,e){switch(e){case B.L:return tt[(t-1)*4+0];case B.M:return tt[(t-1)*4+1];case B.Q:return tt[(t-1)*4+2];case B.H:return tt[(t-1)*4+3];default:return}};dt.getTotalCodewordsCount=function(t,e){switch(e){case B.L:return et[(t-1)*4+0];case B.M:return et[(t-1)*4+1];case B.Q:return et[(t-1)*4+2];case B.H:return et[(t-1)*4+3];default:return}}});var Yt=y(rt=>{var O=new Uint8Array(512),nt=new Uint8Array(256);(function(){let t=1;for(let e=0;e<255;e++)O[e]=t,nt[t]=e,t<<=1,t&256&&(t^=285);for(let e=255;e<512;e++)O[e]=O[e-255]})();rt.log=function(t){if(t<1)throw new Error("log("+t+")");return nt[t]};rt.exp=function(t){return O[t]};rt.mul=function(t,e){return t===0||e===0?0:O[nt[t]+nt[e]]}});var jt=y(K=>{var ht=Yt();K.mul=function(t,e){let r=new Uint8Array(t.length+e.length-1);for(let o=0;o<t.length;o++)for(let i=0;i<e.length;i++)r[o+i]^=ht.mul(t[o],e[i]);return r};K.mod=function(t,e){let r=new Uint8Array(t);for(;r.length-e.length>=0;){let o=r[0];for(let s=0;s<e.length;s++)r[s]^=ht.mul(e[s],o);let i=0;for(;i<r.length&&r[i]===0;)i++;r=r.slice(i)}return r};K.generateECPolynomial=function(t){let e=new Uint8Array([1]);for(let r=0;r<t;r++)e=K.mul(e,new Uint8Array([1,ht.exp(r)]));return e}});var Wt=y((ir,Qt)=>{var Gt=jt();function gt(n){this.genPoly=void 0,this.degree=n,this.degree&&this.initialize(this.degree)}gt.prototype.initialize=function(t){this.degree=t,this.genPoly=Gt.generateECPolynomial(this.degree)};gt.prototype.encode=function(t){if(!this.genPoly)throw new Error("Encoder not initialized");let e=new Uint8Array(t.length+this.degree);e.set(t);let r=Gt.mod(e,this.genPoly),o=this.degree-r.length;if(o>0){let i=new Uint8Array(this.degree);return i.set(r,o),i}return r};Qt.exports=gt});var pt=y(Zt=>{Zt.isValid=function(t){return!isNaN(t)&&t>=1&&t<=40}});var mt=y(S=>{var Xt="[0-9]+",Qe="[A-Z $%*+\\-./:]+",J="(?:[u3000-u303F]|[u3040-u309F]|[u30A0-u30FF]|[uFF00-uFFEF]|[u4E00-u9FAF]|[u2605-u2606]|[u2190-u2195]|u203B|[u2010u2015u2018u2019u2025u2026u201Cu201Du2225u2260]|[u0391-u0451]|[u00A7u00A8u00B1u00B4u00D7u00F7])+";J=J.replace(/u/g,"\\u");var We="(?:(?![A-Z0-9 $%*+\\-./:]|"+J+`)(?:.|[\r
]))+`;S.KANJI=new RegExp(J,"g");S.BYTE_KANJI=new RegExp("[^A-Z0-9 $%*+\\-./:]+","g");S.BYTE=new RegExp(We,"g");S.NUMERIC=new RegExp(Xt,"g");S.ALPHANUMERIC=new RegExp(Qe,"g");var Ze=new RegExp("^"+J+"$"),Xe=new RegExp("^"+Xt+"$"),tn=new RegExp("^[A-Z0-9 $%*+\\-./:]+$");S.testKanji=function(t){return Ze.test(t)};S.testNumeric=function(t){return Xe.test(t)};S.testAlphanumeric=function(t){return tn.test(t)}});var N=y(T=>{var en=pt(),yt=mt();T.NUMERIC={id:"Numeric",bit:1,ccBits:[10,12,14]};T.ALPHANUMERIC={id:"Alphanumeric",bit:2,ccBits:[9,11,13]};T.BYTE={id:"Byte",bit:4,ccBits:[8,16,16]};T.KANJI={id:"Kanji",bit:8,ccBits:[8,10,12]};T.MIXED={bit:-1};T.getCharCountIndicator=function(t,e){if(!t.ccBits)throw new Error("Invalid mode: "+t);if(!en.isValid(e))throw new Error("Invalid version: "+e);return e>=1&&e<10?t.ccBits[0]:e<27?t.ccBits[1]:t.ccBits[2]};T.getBestModeForData=function(t){return yt.testNumeric(t)?T.NUMERIC:yt.testAlphanumeric(t)?T.ALPHANUMERIC:yt.testKanji(t)?T.KANJI:T.BYTE};T.toString=function(t){if(t&&t.id)return t.id;throw new Error("Invalid mode")};T.isValid=function(t){return t&&t.bit&&t.ccBits};function nn(n){if(typeof n!="string")throw new Error("Param is not a string");switch(n.toLowerCase()){case"numeric":return T.NUMERIC;case"alphanumeric":return T.ALPHANUMERIC;case"kanji":return T.KANJI;case"byte":return T.BYTE;default:throw new Error("Unknown mode: "+n)}}T.from=function(t,e){if(T.isValid(t))return t;try{return nn(t)}catch{return e}}});var oe=y(P=>{var ot=M(),rn=ft(),te=Z(),I=N(),wt=pt(),ne=7973,ee=ot.getBCHDigit(ne);function on(n,t,e){for(let r=1;r<=40;r++)if(t<=P.getCapacity(r,e,n))return r}function re(n,t){return I.getCharCountIndicator(n,t)+4}function sn(n,t){let e=0;return n.forEach(function(r){let o=re(r.mode,t);e+=o+r.getBitsLength()}),e}function ln(n,t){for(let e=1;e<=40;e++)if(sn(n,e)<=P.getCapacity(e,t,I.MIXED))return e}P.from=function(t,e){return wt.isValid(t)?parseInt(t,10):e};P.getCapacity=function(t,e,r){if(!wt.isValid(t))throw new Error("Invalid QR Code version");typeof r>"u"&&(r=I.BYTE);let o=ot.getSymbolTotalCodewords(t),i=rn.getTotalCodewordsCount(t,e),s=(o-i)*8;if(r===I.MIXED)return s;let l=s-re(r,t);switch(r){case I.NUMERIC:return Math.floor(l/10*3);case I.ALPHANUMERIC:return Math.floor(l/11*2);case I.KANJI:return Math.floor(l/13);case I.BYTE:default:return Math.floor(l/8)}};P.getBestVersionForData=function(t,e){let r,o=te.from(e,te.M);if(Array.isArray(t)){if(t.length>1)return ln(t,o);if(t.length===0)return 1;r=t[0]}else r=t;return on(r.mode,r.getLength(),o)};P.getEncodedBits=function(t){if(!wt.isValid(t)||t<7)throw new Error("Invalid QR Code version");let e=t<<12;for(;ot.getBCHDigit(e)-ee>=0;)e^=ne<<ot.getBCHDigit(e)-ee;return t<<12|e}});var ce=y(le=>{var bt=M(),se=1335,cn=21522,ie=bt.getBCHDigit(se);le.getEncodedBits=function(t,e){let r=t.bit<<3|e,o=r<<10;for(;bt.getBCHDigit(o)-ie>=0;)o^=se<<bt.getBCHDigit(o)-ie;return(r<<10|o)^cn}});var ue=y((dr,ae)=>{var an=N();function _(n){this.mode=an.NUMERIC,this.data=n.toString()}_.getBitsLength=function(t){return 10*Math.floor(t/3)+(t%3?t%3*3+1:0)};_.prototype.getLength=function(){return this.data.length};_.prototype.getBitsLength=function(){return _.getBitsLength(this.data.length)};_.prototype.write=function(t){let e,r,o;for(e=0;e+3<=this.data.length;e+=3)r=this.data.substr(e,3),o=parseInt(r,10),t.put(o,10);let i=this.data.length-e;i>0&&(r=this.data.substr(e),o=parseInt(r,10),t.put(o,i*3+1))};ae.exports=_});var fe=y((fr,de)=>{var un=N(),Et=["0","1","2","3","4","5","6","7","8","9","A","B","C","D","E","F","G","H","I","J","K","L","M","N","O","P","Q","R","S","T","U","V","W","X","Y","Z"," ","$","%","*","+","-",".","/",":"];function D(n){this.mode=un.ALPHANUMERIC,this.data=n}D.getBitsLength=function(t){return 11*Math.floor(t/2)+6*(t%2)};D.prototype.getLength=function(){return this.data.length};D.prototype.getBitsLength=function(){return D.getBitsLength(this.data.length)};D.prototype.write=function(t){let e;for(e=0;e+2<=this.data.length;e+=2){let r=Et.indexOf(this.data[e])*45;r+=Et.indexOf(this.data[e+1]),t.put(r,11)}this.data.length%2&&t.put(Et.indexOf(this.data[e]),6)};de.exports=D});var ge=y((hr,he)=>{var dn=N();function F(n){this.mode=dn.BYTE,typeof n=="string"?this.data=new TextEncoder().encode(n):this.data=new Uint8Array(n)}F.getBitsLength=function(t){return t*8};F.prototype.getLength=function(){return this.data.length};F.prototype.getBitsLength=function(){return F.getBitsLength(this.data.length)};F.prototype.write=function(n){for(let t=0,e=this.data.length;t<e;t++)n.put(this.data[t],8)};he.exports=F});var me=y((gr,pe)=>{var fn=N(),hn=M();function v(n){this.mode=fn.KANJI,this.data=n}v.getBitsLength=function(t){return t*13};v.prototype.getLength=function(){return this.data.length};v.prototype.getBitsLength=function(){return v.getBitsLength(this.data.length)};v.prototype.write=function(n){let t;for(t=0;t<this.data.length;t++){let e=hn.toSJIS(this.data[t]);if(e>=33088&&e<=40956)e-=33088;else if(e>=57408&&e<=60351)e-=49472;else throw new Error("Invalid SJIS character: "+this.data[t]+`
Make sure your charset is UTF-8`);e=(e>>>8&255)*192+(e&255),n.put(e,13)}};pe.exports=v});var ye=y((pr,Tt)=>{"use strict";var Y={single_source_shortest_paths:function(n,t,e){var r={},o={};o[t]=0;var i=Y.PriorityQueue.make();i.push(t,0);for(var s,l,c,f,a,m,d,u,h;!i.empty();){s=i.pop(),l=s.value,f=s.cost,a=n[l]||{};for(c in a)a.hasOwnProperty(c)&&(m=a[c],d=f+m,u=o[c],h=typeof o[c]>"u",(h||u>d)&&(o[c]=d,i.push(c,d),r[c]=l))}if(typeof e<"u"&&typeof o[e]>"u"){var p=["Could not find a path from ",t," to ",e,"."].join("");throw new Error(p)}return r},extract_shortest_path_from_predecessor_list:function(n,t){for(var e=[],r=t,o;r;)e.push(r),o=n[r],r=n[r];return e.reverse(),e},find_path:function(n,t,e){var r=Y.single_source_shortest_paths(n,t,e);return Y.extract_shortest_path_from_predecessor_list(r,e)},PriorityQueue:{make:function(n){var t=Y.PriorityQueue,e={},r;n=n||{};for(r in t)t.hasOwnProperty(r)&&(e[r]=t[r]);return e.queue=[],e.sorter=n.sorter||t.default_sorter,e},default_sorter:function(n,t){return n.cost-t.cost},push:function(n,t){var e={value:n,cost:t};this.queue.push(e),this.queue.sort(this.sorter)},pop:function(){return this.queue.shift()},empty:function(){return this.queue.length===0}}};typeof Tt<"u"&&(Tt.exports=Y)});var Se=y(z=>{var g=N(),Ee=ue(),Te=fe(),Ce=ge(),xe=me(),j=mt(),it=M(),gn=ye();function we(n){return unescape(encodeURIComponent(n)).length}function G(n,t,e){let r=[],o;for(;(o=n.exec(e))!==null;)r.push({data:o[0],index:o.index,mode:t,length:o[0].length});return r}function Ae(n){let t=G(j.NUMERIC,g.NUMERIC,n),e=G(j.ALPHANUMERIC,g.ALPHANUMERIC,n),r,o;return it.isKanjiModeEnabled()?(r=G(j.BYTE,g.BYTE,n),o=G(j.KANJI,g.KANJI,n)):(r=G(j.BYTE_KANJI,g.BYTE,n),o=[]),t.concat(e,r,o).sort(function(s,l){return s.index-l.index}).map(function(s){return{data:s.data,mode:s.mode,length:s.length}})}function Ct(n,t){switch(t){case g.NUMERIC:return Ee.getBitsLength(n);case g.ALPHANUMERIC:return Te.getBitsLength(n);case g.KANJI:return xe.getBitsLength(n);case g.BYTE:return Ce.getBitsLength(n)}}function pn(n){return n.reduce(function(t,e){let r=t.length-1>=0?t[t.length-1]:null;return r&&r.mode===e.mode?(t[t.length-1].data+=e.data,t):(t.push(e),t)},[])}function mn(n){let t=[];for(let e=0;e<n.length;e++){let r=n[e];switch(r.mode){case g.NUMERIC:t.push([r,{data:r.data,mode:g.ALPHANUMERIC,length:r.length},{data:r.data,mode:g.BYTE,length:r.length}]);break;case g.ALPHANUMERIC:t.push([r,{data:r.data,mode:g.BYTE,length:r.length}]);break;case g.KANJI:t.push([r,{data:r.data,mode:g.BYTE,length:we(r.data)}]);break;case g.BYTE:t.push([{data:r.data,mode:g.BYTE,length:we(r.data)}])}}return t}function yn(n,t){let e={},r={start:{}},o=["start"];for(let i=0;i<n.length;i++){let s=n[i],l=[];for(let c=0;c<s.length;c++){let f=s[c],a=""+i+c;l.push(a),e[a]={node:f,lastCount:0},r[a]={};for(let m=0;m<o.length;m++){let d=o[m];e[d]&&e[d].node.mode===f.mode?(r[d][a]=Ct(e[d].lastCount+f.length,f.mode)-Ct(e[d].lastCount,f.mode),e[d].lastCount+=f.length):(e[d]&&(e[d].lastCount=f.length),r[d][a]=Ct(f.length,f.mode)+4+g.getCharCountIndicator(f.mode,t))}}o=l}for(let i=0;i<o.length;i++)r[o[i]].end=0;return{map:r,table:e}}function be(n,t){let e,r=g.getBestModeForData(n);if(e=g.from(t,r),e!==g.BYTE&&e.bit<r.bit)throw new Error('"'+n+'" cannot be encoded with mode '+g.toString(e)+`.
 Suggested mode is: `+g.toString(r));switch(e===g.KANJI&&!it.isKanjiModeEnabled()&&(e=g.BYTE),e){case g.NUMERIC:return new Ee(n);case g.ALPHANUMERIC:return new Te(n);case g.KANJI:return new xe(n);case g.BYTE:return new Ce(n)}}z.fromArray=function(t){return t.reduce(function(e,r){return typeof r=="string"?e.push(be(r,null)):r.data&&e.push(be(r.data,r.mode)),e},[])};z.fromString=function(t,e){let r=Ae(t,it.isKanjiModeEnabled()),o=mn(r),i=yn(o,e),s=gn.find_path(i.map,"start","end"),l=[];for(let c=1;c<s.length-1;c++)l.push(i.table[s[c]].node);return z.fromArray(pn(l))};z.rawSplit=function(t){return z.fromArray(Ae(t,it.isKanjiModeEnabled()))}});var Be=y(Me=>{var lt=M(),xt=Z(),wn=Ft(),bn=zt(),En=Vt(),Tn=Kt(),Mt=Jt(),Bt=ft(),Cn=Wt(),st=oe(),xn=ce(),An=N(),At=Se();function Sn(n,t){let e=n.size,r=Tn.getPositions(t);for(let o=0;o<r.length;o++){let i=r[o][0],s=r[o][1];for(let l=-1;l<=7;l++)if(!(i+l<=-1||e<=i+l))for(let c=-1;c<=7;c++)s+c<=-1||e<=s+c||(l>=0&&l<=6&&(c===0||c===6)||c>=0&&c<=6&&(l===0||l===6)||l>=2&&l<=4&&c>=2&&c<=4?n.set(i+l,s+c,!0,!0):n.set(i+l,s+c,!1,!0))}}function Mn(n){let t=n.size;for(let e=8;e<t-8;e++){let r=e%2===0;n.set(e,6,r,!0),n.set(6,e,r,!0)}}function Bn(n,t){let e=En.getPositions(t);for(let r=0;r<e.length;r++){let o=e[r][0],i=e[r][1];for(let s=-2;s<=2;s++)for(let l=-2;l<=2;l++)s===-2||s===2||l===-2||l===2||s===0&&l===0?n.set(o+s,i+l,!0,!0):n.set(o+s,i+l,!1,!0)}}function Nn(n,t){let e=n.size,r=st.getEncodedBits(t),o,i,s;for(let l=0;l<18;l++)o=Math.floor(l/3),i=l%3+e-8-3,s=(r>>l&1)===1,n.set(o,i,s,!0),n.set(i,o,s,!0)}function St(n,t,e){let r=n.size,o=xn.getEncodedBits(t,e),i,s;for(i=0;i<15;i++)s=(o>>i&1)===1,i<6?n.set(i,8,s,!0):i<8?n.set(i+1,8,s,!0):n.set(r-15+i,8,s,!0),i<8?n.set(8,r-i-1,s,!0):i<9?n.set(8,15-i-1+1,s,!0):n.set(8,15-i-1,s,!0);n.set(r-8,8,1,!0)}function In(n,t){let e=n.size,r=-1,o=e-1,i=7,s=0;for(let l=e-1;l>0;l-=2)for(l===6&&l--;;){for(let c=0;c<2;c++)if(!n.isReserved(o,l-c)){let f=!1;s<t.length&&(f=(t[s]>>>i&1)===1),n.set(o,l-c,f),i--,i===-1&&(s++,i=7)}if(o+=r,o<0||e<=o){o-=r,r=-r;break}}}function Ln(n,t,e){let r=new wn;e.forEach(function(c){r.put(c.mode.bit,4),r.put(c.getLength(),An.getCharCountIndicator(c.mode,n)),c.write(r)});let o=lt.getSymbolTotalCodewords(n),i=Bt.getTotalCodewordsCount(n,t),s=(o-i)*8;for(r.getLengthInBits()+4<=s&&r.put(0,4);r.getLengthInBits()%8!==0;)r.putBit(0);let l=(s-r.getLengthInBits())/8;for(let c=0;c<l;c++)r.put(c%2?17:236,8);return qn(r,n,t)}function qn(n,t,e){let r=lt.getSymbolTotalCodewords(t),o=Bt.getTotalCodewordsCount(t,e),i=r-o,s=Bt.getBlocksCount(t,e),l=r%s,c=s-l,f=Math.floor(r/s),a=Math.floor(i/s),m=a+1,d=f-a,u=new Cn(d),h=0,p=new Array(s),C=new Array(s),k=0,W=new Uint8Array(n.buffer);for(let A=0;A<s;A++){let V=A<c?a:m;p[A]=W.slice(h,h+V),C[A]=u.encode(p[A]),h+=V,k=Math.max(k,V)}let U=new Uint8Array(r),L=0,b,E;for(b=0;b<k;b++)for(E=0;E<s;E++)b<p[E].length&&(U[L++]=p[E][b]);for(b=0;b<d;b++)for(E=0;E<s;E++)U[L++]=C[E][b];return U}function Rn(n,t,e,r){let o;if(Array.isArray(n))o=At.fromArray(n);else if(typeof n=="string"){let f=t;if(!f){let a=At.rawSplit(n);f=st.getBestVersionForData(a,e)}o=At.fromString(n,f||40)}else throw new Error("Invalid data");let i=st.getBestVersionForData(o,e);if(!i)throw new Error("The amount of data is too big to be stored in a QR Code");if(!t)t=i;else if(t<i)throw new Error(`
The chosen QR Code version cannot contain this amount of data.
Minimum version required to store current data is: `+i+`.
`);let s=Ln(t,e,o),l=lt.getSymbolSize(t),c=new bn(l);return Sn(c,t),Mn(c),Bn(c,t),St(c,e,0),t>=7&&Nn(c,t),In(c,s),isNaN(r)&&(r=Mt.getBestMask(c,St.bind(null,c,e))),Mt.applyMask(r,c),St(c,e,r),{modules:c,version:t,errorCorrectionLevel:e,maskPattern:r,segments:o}}Me.create=function(t,e){if(typeof t>"u"||t==="")throw new Error("No input text");let r=xt.M,o,i;return typeof e<"u"&&(r=xt.from(e.errorCorrectionLevel,xt.M),o=st.from(e.version),i=Mt.from(e.maskPattern),e.toSJISFunc&&lt.setToSJISFunction(e.toSJISFunc)),Rn(t,o,r,i)}});var Nt=y(H=>{function Ne(n){if(typeof n=="number"&&(n=n.toString()),typeof n!="string")throw new Error("Color should be defined as hex string");let t=n.slice().replace("#","").split("");if(t.length<3||t.length===5||t.length>8)throw new Error("Invalid hex color: "+n);(t.length===3||t.length===4)&&(t=Array.prototype.concat.apply([],t.map(function(r){return[r,r]}))),t.length===6&&t.push("F","F");let e=parseInt(t.join(""),16);return{r:e>>24&255,g:e>>16&255,b:e>>8&255,a:e&255,hex:"#"+t.slice(0,6).join("")}}H.getOptions=function(t){t||(t={}),t.color||(t.color={});let e=typeof t.margin>"u"||t.margin===null||t.margin<0?4:t.margin,r=t.width&&t.width>=21?t.width:void 0,o=t.scale||4;return{width:r,scale:r?4:o,margin:e,color:{dark:Ne(t.color.dark||"#000000ff"),light:Ne(t.color.light||"#ffffffff")},type:t.type,rendererOpts:t.rendererOpts||{}}};H.getScale=function(t,e){return e.width&&e.width>=t+e.margin*2?e.width/(t+e.margin*2):e.scale};H.getImageWidth=function(t,e){let r=H.getScale(t,e);return Math.floor((t+e.margin*2)*r)};H.qrToImageData=function(t,e,r){let o=e.modules.size,i=e.modules.data,s=H.getScale(o,r),l=Math.floor((o+r.margin*2)*s),c=r.margin*s,f=[r.color.light,r.color.dark];for(let a=0;a<l;a++)for(let m=0;m<l;m++){let d=(a*l+m)*4,u=r.color.light;if(a>=c&&m>=c&&a<l-c&&m<l-c){let h=Math.floor((a-c)/s),p=Math.floor((m-c)/s);u=f[i[h*o+p]?1:0]}t[d++]=u.r,t[d++]=u.g,t[d++]=u.b,t[d]=u.a}}});var Ie=y(ct=>{var It=Nt();function Pn(n,t,e){n.clearRect(0,0,t.width,t.height),t.style||(t.style={}),t.height=e,t.width=e,t.style.height=e+"px",t.style.width=e+"px"}function Hn(){try{return document.createElement("canvas")}catch{throw new Error("You need to specify a canvas element")}}ct.render=function(t,e,r){let o=r,i=e;typeof o>"u"&&(!e||!e.getContext)&&(o=e,e=void 0),e||(i=Hn()),o=It.getOptions(o);let s=It.getImageWidth(t.modules.size,o),l=i.getContext("2d"),c=l.createImageData(s,s);return It.qrToImageData(c.data,t,o),Pn(l,i,s),l.putImageData(c,0,0),i};ct.renderToDataURL=function(t,e,r){let o=r;typeof o>"u"&&(!e||!e.getContext)&&(o=e,e=void 0),o||(o={});let i=ct.render(t,e,o),s=o.type||"image/png",l=o.rendererOpts||{};return i.toDataURL(s,l.quality)}});var Re=y(qe=>{var kn=Nt();function Le(n,t){let e=n.a/255,r=t+'="'+n.hex+'"';return e<1?r+" "+t+'-opacity="'+e.toFixed(2).slice(1)+'"':r}function Lt(n,t,e){let r=n+t;return typeof e<"u"&&(r+=" "+e),r}function Un(n,t,e){let r="",o=0,i=!1,s=0;for(let l=0;l<n.length;l++){let c=Math.floor(l%t),f=Math.floor(l/t);!c&&!i&&(i=!0),n[l]?(s++,l>0&&c>0&&n[l-1]||(r+=i?Lt("M",c+e,.5+f+e):Lt("m",o,0),o=0,i=!1),c+1<t&&n[l+1]||(r+=Lt("h",s),s=0)):o++}return r}qe.render=function(t,e,r){let o=kn.getOptions(e),i=t.modules.size,s=t.modules.data,l=i+o.margin*2,c=o.color.light.a?"<path "+Le(o.color.light,"fill")+' d="M0 0h'+l+"v"+l+'H0z"/>':"",f="<path "+Le(o.color.dark,"stroke")+' d="'+Un(s,i,o.margin)+'"/>',a='viewBox="0 0 '+l+" "+l+'"',d='<svg xmlns="http://www.w3.org/2000/svg" '+(o.width?'width="'+o.width+'" height="'+o.width+'" ':"")+a+' shape-rendering="crispEdges">'+c+f+`</svg>
`;return typeof r=="function"&&r(null,d),d}});var He=y(Q=>{var _n=Ut(),qt=Be(),Pe=Ie(),Dn=Re();function Rt(n,t,e,r,o){let i=[].slice.call(arguments,1),s=i.length,l=typeof i[s-1]=="function";if(!l&&!_n())throw new Error("Callback required as last argument");if(l){if(s<2)throw new Error("Too few arguments provided");s===2?(o=e,e=t,t=r=void 0):s===3&&(t.getContext&&typeof o>"u"?(o=r,r=void 0):(o=r,r=e,e=t,t=void 0))}else{if(s<1)throw new Error("Too few arguments provided");return s===1?(e=t,t=r=void 0):s===2&&!t.getContext&&(r=e,e=t,t=void 0),new Promise(function(c,f){try{let a=qt.create(e,r);c(n(a,t,r))}catch(a){f(a)}})}try{let c=qt.create(e,r);o(null,n(c,t,r))}catch(c){o(c)}}Q.create=qt.create;Q.toCanvas=Rt.bind(null,Pe.render);Q.toDataURL=Rt.bind(null,Pe.renderToDataURL);Q.toString=Rt.bind(null,function(n,t,e){return Dn.render(n,e)})});var Ue=Oe(He(),1);var Fn=`* {
  margin: 0;
  padding: 0;
}

body {
  width: 21cm;
  height: 29.7cm;
}

@font-face {
  font-family: "Times New Roman";
  src: url("TIMES.TTF") format("truetype");
  font-weight: 400;
  font-style: normal;
}
@font-face {
  font-family: "Times New Roman";
  src: url("TIMESBD.TTF") format("truetype");
  font-weight: 700;
  font-style: normal;
}
@font-face {
  font-family: "Times New Roman";
  src: url("TIMESI.TTF") format("truetype");
  font-weight: 400;
  font-style: italic;
}
@font-face {
  font-family: "Times New Roman";
  src: url("TIMESBI.TTF") format("truetype");
  font-weight: 700;
  font-style: italic;
}
* {
  font-feature-settings: "tnum", "tnum";
  font-family: "Times New Roman";
  font-variant: tabular-nums;
  list-style: none;
  pointer-events: auto;
  word-wrap: break-word;
}

.data-item {
  margin-bottom: 0;
}

.data-item .di-label {
  color: rgba(0, 0, 0, 0.85);
  font-size: 17.33333px;
  line-height: 1.5;
  list-style: none;
  font-size: 13pt;
  color: rgba(0, 0, 0, 0.85);
  box-sizing: border-box;
  min-height: 25px;
  height: auto;
  border-bottom: 1px dashed transparent;
  display: flex;
  align-items: flex-start;
}

.data-item .di-value {
  line-height: 1.5;
  list-style: none;
  word-break: break-word;
  font-size: 13pt;
  color: rgba(0, 0, 0, 0.85);
  box-sizing: border-box;
  flex: 1 1 0%;
  min-height: 25px;
  display: flex;
  align-items: flex-start;
  padding-left: 5px;
  height: auto;
  -webkit-box-pack: unset;
  justify-content: unset;
  border-bottom: none;
  font-weight: 600;
}

.code-ms {
  color: black;
  line-height: 1.5;
  box-sizing: border-box;
  font-weight: bolder;
  display: flex;
  font-size: 12pt;
}

.res-tb {
  font-size: 14px;
  word-wrap: break-word;
  color: black;
  line-height: 1.5;
  box-sizing: border-box;
  border-collapse: collapse;
  border-spacing: 0px;
  width: 100%;
  overflow-x: auto;
  margin: 10px 0px;
  min-width: 250px;
}

.res-tb td {
  border: 1px solid black;
  padding: 6px 4px;
  vertical-align: baseline;
}

.res-tb td.tx-center {
  font-size: 14px;
  word-wrap: break-word;
  color: black;
  line-height: 1.5;
  border-collapse: collapse;
  border-spacing: 0px;
  box-sizing: border-box;
  border: 1px solid black;
  padding: 6px 4px;
  vertical-align: baseline;
  text-align: center;
  min-width: 50px;
}

.tb-stt {
  width: auto;
}

.di-value,
.di-label,
.day {
  font-size: 14px !important;
  min-height: 0px !important;
}

.heading-content {
  position: relative;
}

.top-content {
  position: absolute;
  width: 100%;
}
.top-content .code-content {
  font-size: 14px;
}

.main-page {
  border: 0;
  padding: 24px;
}
.main-page > div {
  border: 3px double rgba(145, 87, 21, 0.69);
  padding: 8px 16px;
  border-width: unset;
}`;document.head.appendChild(document.createElement("style")).appendChild(document.createTextNode(Fn));var ke={width:21,height:29.7};await vn();async function vn(){await Promise.all([zn(),new Promise(async n=>{await document.fonts.load('16px "Times New Roman"'),await document.fonts.ready,n()})]),Vn(),$n(),On(),Kn(),at(document.querySelector("body>div"),"0/0"),document.querySelectorAll(".fd-end").forEach(n=>n.remove()),Jn()}async function zn(){let n=document.querySelector("#qrcodeContent").value;if(n!==""){let t=await Ue.toDataURL(n,{errorCorrectionLevel:"low",width:80}),e=new Image;e.src=t;let r=document.getElementById("qrcodeTable");r.innerHTML="",r.appendChild(e)}}function Vn(){let n=document.getElementById("cks").innerText,t=n.trim().replace(/[^,\s=]+=/,"").split(/,*\s*[^,\s=]+=/),r=n.match(/[^,\s=]+(?==)/g).reduce((o,i,s)=>(o[i]=t[s],o),{});document.getElementById("cks").innerText=r.CN||""}function $n(n=["T\xEDnh ch\u1EA5t","Lo\u1EA1i h\xE0ng h\xF3a \u0111\u1EB7c tr\u01B0ng","Chi\u1EBFt kh\u1EA5u"]){let t=document.querySelector("table"),e=t.querySelector("thead"),r=Array.from(e.querySelectorAll("th")),o=r.map((u,h)=>{let p=u.textContent.trim();return n.includes(p)?h:null}).filter(u=>u!==null);o.forEach(u=>{let h=r[u];h&&h.remove()});let i=t.querySelector("tbody");Array.from(i.querySelectorAll("tr")).forEach(u=>{let h=Array.from(u.children);o.forEach(p=>{h[p]&&h[p].remove()})}),i.querySelectorAll("tr > td.tx-left:nth-child(3)").forEach(u=>u.className="tx-center"),i.querySelectorAll("tr > td.tx-left:nth-child(2)").forEach(u=>u.style.minWidth=u.style.maxWidth="220px"),r=Array.from(e.querySelectorAll("th"));let l=r.indexOf(e.querySelector(".tb-ts")),c=e.querySelector(".tb-ts");c.parentNode.appendChild(c),i.querySelectorAll("tr").forEach(u=>{let h=[...u.querySelectorAll("td")];u.appendChild(h[l])}),r=Array.from(e.querySelectorAll("th")),l=r.indexOf(e.querySelector(".tb-ts")),r=Array.from(e.querySelectorAll("th"));let f=r.indexOf(e.querySelector(".tb-ttct")),a=document.createElement("th");a.className="tb-ttct tb-tt",a.textContent="Ti\u1EC1n thu\u1EBF",c.parentNode.appendChild(a),i.querySelectorAll("tr").forEach(u=>{let h=u.getAttribute("t-chat")==="1"||u.getAttribute("t-chat")==="5",p=document.createElement("td");if(p.style.textAlign="right",h){let C=[...u.querySelectorAll("td")];p.textContent=new Intl.NumberFormat("vi-VN").format(parseFloat(decodeURIComponent(C[l].textContent.trim()).trim().replace(/\./g,"").replace(/,/,".").replace("%",""))*parseFloat(decodeURIComponent(C[f].textContent.trim()).trim().replace(/\./g,"").replace(/,/,"."))/100)}u.appendChild(p)}),r=Array.from(e.querySelectorAll("th"));let m=r.indexOf(e.querySelector(".tb-tt")),d=document.createElement("th");d.className="tb-ttct",d.textContent="Th\xE0nh ti\u1EC1n sau thu\u1EBF",c.parentNode.appendChild(d),i.querySelectorAll("tr").forEach(u=>{let h=u.getAttribute("t-chat")==="1"||u.getAttribute("t-chat")==="5",p=document.createElement("td");if(p.style.textAlign="right",h){let C=[...u.querySelectorAll("td")];p.textContent=new Intl.NumberFormat("vi-VN").format(parseFloat(decodeURIComponent(C[m].textContent.trim()).trim().replace(/\./g,"").replace(/,/,"."))+parseFloat(decodeURIComponent(C[f].textContent.trim()).trim().replace(/\./g,"").replace(/,/,".")))}u.appendChild(p)}),e.querySelector(".tb-dg").style.width="250px",e.querySelector(".tb-ttct").textContent="Th\xE0nh ti\u1EC1n"}function On(n=["T\u1ED5ng ti\u1EC1n ph\xED","T\u1ED5ng ti\u1EC1n chi\u1EBFt kh\u1EA5u th\u01B0\u01A1ng m\u1EA1i"]){let t=Array.from(document.querySelectorAll("table")).at(-2),e=Array.from(t.querySelectorAll("tbody > tr")).map(b=>{let E=Array.from(b.querySelectorAll("td")),A=E[0].textContent,V=E[1].textContent,_e=E[2].textContent;return{per:A,amount:V,tax:_e}}),r=Array.from(document.querySelectorAll("table")).at(-1),o=r.querySelector("tbody"),i=Array.from(o.querySelectorAll("tr"));i.map((b,E)=>{let A=b.querySelector(".tx-center")?.textContent.trim();return A&&n.includes(A)?E:null}).filter(b=>b!==null).forEach(b=>{let E=i[b];E&&E.remove()});function l(b){return i.find(E=>E.querySelector(".tx-center")?.textContent.trim().includes(b))}let c=l("T\u1ED5ng ti\u1EC1n chi\u1EBFt kh\u1EA5u th\u01B0\u01A1ng m\u1EA1i")?.querySelector(".tx-center:last-child")?.textContent,f=l("T\u1ED5ng ti\u1EC1n ch\u01B0a thu\u1EBF")?.querySelector(".tx-center:last-child")?.textContent,a=l("T\u1ED5ng ti\u1EC1n thu\u1EBF")?.querySelector(".tx-center:last-child")?.textContent,m=l("T\u1ED5ng ti\u1EC1n thanh to\xE1n b\u1EB1ng s\u1ED1")?.querySelector(".tx-center:last-child")?.textContent,d=l("T\u1ED5ng ti\u1EC1n thanh to\xE1n b\u1EB1ng ch\u1EEF")?.querySelector(".tx-center:last-child")?.textContent,u=document.querySelector("table > tbody"),h=u.querySelectorAll("tr:first-child > td").length,p=c&&c.trim()!=="0"?document.createElement("tr"):null;p&&(p.innerHTML=`
<td colspan="${h-1}" class="tx-center" style="text-align: right"><b>T\u1ED5ng ti\u1EC1n chi\u1EBFt kh\u1EA5u th\u01B0\u01A1ng m\u1EA1i: </b></td>
<td class="tx-center">${c}</td>
  `);let C=document.createElement("tr");C.innerHTML=`
<td colspan="${h-4}" class="tx-center" style="text-align: right"><b>C\u1ED9ng ti\u1EC1n h\xE0ng h\xF3a, d\u1ECBch v\u1EE5: </b></td>
<td class="tx-center">${f}</td>
<td />
<td class="tx-center">${a}</td>
<td rowspan="2" class="tx-center" style="vertical-align: middle"><b>${m}<b></td>
  `;let k=document.createElement("tr"),W=e.length===1;k.innerHTML=`
${W?`
<td colspan="${Math.floor((h-1)/2)}" class="text-left"><b>Thu\u1EBF xu\u1EA5t GTGT: </b>${e[0].per}</td>
`:""}
<td colspan="${W?Math.ceil((h-1)/2):h-1}" class="text-center" style="text-align: right"><b>T\u1ED5ng c\u1ED9ng ti\u1EC1n thanh to\xE1n: </b></td>
`;let U=document.createElement("tr");U.innerHTML=`
<td colspan="${h}" class="tx-left">
<b>S\u1ED1 ti\u1EC1n vi\u1EBFt b\u1EB1ng ch\u1EEF:</b>
${d}</td>
`;let L=e.length>1?document.createElement("tr"):null;L&&(L.innerHTML=`
<td colspan="${h}">
  <ul style="display: flex; flex-wrap: wrap">
    ${e.map(b=>`<li style="width: 50%; display: inline-block">T\u1ED5ng ti\u1EC1n ch\u1ECBu thu\u1EBF ${b.per}: <b>${b.tax}</b></li>`).join(`
`)}
  </ul>
</td>
`),p&&u.appendChild(p),u.appendChild(C),u.appendChild(k),u.appendChild(U),L&&u.appendChild(L),console.log({totalAmountWithoutTax:f,totalTax:a,totalAll:m,totalAllText:d}),r.remove(),console.log(e),t.remove()}function Kn(n=["M\xE3 c\u1EEDa h\xE0ng:","T\xEAn c\u1EEDa h\xE0ng:","S\u1ED1 h\u1ED9 chi\u1EBFu:","M\xE3 \u0110VCQHVNSNN:","CCCD ng\u01B0\u1EDDi mua:","S\u1ED1 b\u1EA3ng k\xEA:","Ng\xE0y b\u1EA3ng k\xEA:"]){document.querySelectorAll(".data-item").forEach(e=>{let r=e.querySelector(".di-label")?.textContent?.trim(),o=e.querySelector(".di-value")?.textContent?.trim();n.includes(r||"")&&o===""&&e.remove()})}function at(n,t){let e=n.querySelector("#paginate"),r=e??document.createElement("div");r.setAttribute("id","paginate"),Object.assign(r.style,{"text-align":"center","font-size":"14px",color:"rgba(0,0,0,0.8)"}),r.innerHTML=(t?`Trang ${t}<br> `:"")+"<i>(C\u1EA7n ki\u1EC3m tra, \u0111\u1ED1i chi\u1EBFu khi l\u1EADp, nh\u1EADn h\xF3a \u0111\u01A1n https://hoadondientu.gdt.gov.vn/)</i>",e||n.append(r)}function Jn(){let n=document.querySelector("body > div");n.setAttribute("id","page"),n.innerHTML=`<div>${n.innerHTML}</div>`;let t=n.children[0];Object.assign(n.style,{width:`${ke.width}cm`,height:`${ke.height}cm`,overflow:"scroll"});let e=n.cloneNode(!0);Pt(n);let r=n.querySelector("table"),o=r.querySelector("tbody"),i=Array.from(o.querySelectorAll("tr"));r.offsetHeight,n.offsetHeight;let s=[],l=[];for(;i.length>0;){for(;n.scrollHeight>n.offsetHeight;){let a=i.at(-1),m=a.cloneNode(!0);if(a.remove(),l.push(m),i.pop(),!i.length)break}s.push(Array.from(o.querySelectorAll("tr")).map(a=>a.cloneNode(!0))),i=l,l=[],o.querySelectorAll("tr").forEach(a=>a.remove()),i.reverse().forEach(a=>o.append(a))}let c=document.createElement("div");document.body.appendChild(c),n.remove();let f=0;s.forEach((a,m)=>{let d=e.cloneNode(!0),u=m===s.length-1;u||(Pt(d),[...d.querySelectorAll(".vip-divide")].at(-1)?.remove());let h=d.querySelector("tbody");if(h.innerHTML="",a.forEach(p=>h.appendChild(p.cloneNode(!0))),d.style.pageBreakAfter="always",c.appendChild(d),u&&d.scrollHeight>d.offsetHeight){console.log("Last page too tall \u2014 splitting..."),Pt(d),f=1;let C=e.cloneNode(!0);C.querySelector("table")?.remove(),c.appendChild(C)}s.length<2?at(d):at(d,`${m+1} / ${s.length+f}`)}),document.querySelectorAll("#page").forEach((a,m,{length:d})=>{at(a,`${m+1} / ${d}`),a.style.position="relative",a.children[0].style.height="100%",a.children[0].style.position="relative",Object.assign(a.querySelector("#paginate").style,{position:"absolute",bottom:"3px",left:"0px",right:"0px",margin:"0px",padding:0}),a.style.overflow="hidden"}),document.body.style.margin="0"}function Pt(n){n.querySelectorAll(".table-horizontal-wrapper, .ft-sign").forEach(t=>t.remove())}
