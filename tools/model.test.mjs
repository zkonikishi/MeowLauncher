import test from 'node:test';
import assert from 'node:assert/strict';
import {validateCredentials,settingsFromInputs,errorMessage} from '../apps/launcher/ui/model.mjs';
test('validation accepts UID only and rejects empty password',()=>{assert.equal(validateCredentials('12345678','x'),null);assert.ok(validateCredentials('a@b.com','x'));assert.ok(validateCredentials('123','x'));assert.ok(validateCredentials('a@b.com',''));});
test('remember selection controls persistence; no password field exists',()=>{const s=settingsFromInputs(' D:\\游戏 ','retro',false,'12345678');assert.equal(s.identifier,'');assert.equal(s.clientDirectory,'D:\\游戏');assert.deepEqual(Object.keys(s).sort(),['clientDirectory','identifier','rememberIdentifier','theme','version']);});
test('unexpected IPC exceptions do not expose raw messages',()=>{assert.ok(!errorMessage(new Error('secret-payload')).includes('secret-payload'));});
