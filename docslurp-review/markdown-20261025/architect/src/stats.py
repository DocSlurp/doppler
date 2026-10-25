#!/usr/bin/env python3
import json,sys,collections
f=sys.argv[1]; st=set(); pages=0; unf=0; words=0; mcw=0; join=0; art=0; imb=collections.Counter(); roles=collections.Counter()
rows=[]
for l in open(f):
    d=json.loads(l); t=d['t']
    if t=='struct': st.add((d['page'],d['mcid'])); roles[d['path'].split('/')[-1]]+=1; rows.append(d)
    elif t=='page': pages+=1; cur=d['n']; unf+= d['mcUnderflow']>0; imb[d['mcEnd']-d['mcBegin']]+=1
    elif t=='line':
        for w in d['words']:
            words+=1; mcw+= w['mc']>=0; art+=w['art']; join+= (cur,w['mc']) in st
print(json.dumps(dict(file=f.split('/')[-1],pages=pages,pages_with_end_without_begin=unf,words=words,words_with_mcid=mcw,words_joined_to_struct=join,artifact_words=art,struct_leaves=len(st),top_roles=roles.most_common(6)),indent=1))
