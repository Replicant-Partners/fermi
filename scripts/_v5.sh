#!/bin/sh
set -a; . ./.env; set +a
AID=$(psql "$DATABASE_URL" -tAc "select agent_id from agents where agent_name='weather_oracle' limit 1" | tr -d ' ')
echo "weather_oracle agent_id = $AID"
TOK=$(python3 -c "
import base64,hmac,hashlib,json,time
S=b'insecure-dev-secret-change-me-in-production'
b=lambda d: base64.urlsafe_b64encode(d).rstrip(b'=')
n=int(time.time())
h={'alg':'HS256','typ':'JWT'}
c={'sub':'2e644008-f5c7-47c5-854c-3801df9879cc','email':'ivan@axolotl.partners','name':'Ivan','role':'admin','provider':'google','github_username':None,'google_id':'dev','exp':n+7200,'iat':n}
s=b(json.dumps(h).encode())+b'.'+b(json.dumps(c).encode())
print((s+b'.'+b(hmac.new(S,s,hashlib.sha256).digest())).decode())")
curl -s -H "Authorization: Bearer $TOK" \
  "http://localhost:3000/api/observatory/agents/$AID/loops" -o /tmp/agl.json
echo "--- keys ---"
python3 -c "
import json;d=json.load(open('/tmp/agl.json'))
print(list(d.keys()))
print(json.dumps(d,indent=2)[:3000])"
