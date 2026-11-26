# Schema decision audit after allocator routing

The [original schema ledger](schema.md) defines evidence groups S1–S7
and the meanings of the status letters below. The current source differs
from that ledger at five field-map enumerations. Each now requests owned
rows through the database allocator. Both accessors require a map and
visit its entries in order. An allocation refusal can stop enumeration
before a field is visited.

Comparison of both inventories matched every owner, decision kind,
alternative and condition. Byte positions below include all preceding
method-name changes. The five changed error-propagation sites have
individual named mutations and a refusal sweep in `tests/schema.rs`.
The remaining 171 rows retain the scope of the original evidence.
In particular, D rows delegate an error to the engine and I rows retain
their structural proof. Neither claims a new executable independence pair.

| Source | SHA-256 |
| --- | --- |
| `src/schema.rs` | `66bb396906513bf771b53627bbfeccc9da8a719bb3667df996677398f17170b8` |

| Status | Target | Position | Bytes | Kind | Owner | Original evidence group |
| --- | --- | --- | --- | --- | --- | --- |
| E | weekend-db::weekend_db | src/schema.rs:63:23– | 2068..2069 | try_propagation | required | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:66:25– | 2184..2185 | try_propagation | unsigned | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:71:83– | 2378..2379 | try_propagation | field_id | S1/S2 |
| P | weekend-db::weekend_db | src/schema.rs:72:5– | 2386..2388 | if | field_id | S1 |
| E | weekend-db::weekend_db | src/schema.rs:100:45– | 3052..3053 | try_propagation | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:101:74– | 3128..3129 | try_propagation | parse | S1 |
| P | weekend-db::weekend_db | src/schema.rs:102:9– | 3139..3141 | if | parse | S1 |
| C | weekend-db::weekend_db | src/schema.rs:102:12– | 3142..3197 | short_circuit_or | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:105:20– | 3279..3284 | match | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:105:51– | 3310..3311 | try_propagation | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:115:53– | 3646..3647 | try_propagation | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:117:51– | 3721..3722 | try_propagation | parse | S1 |
| E | weekend-db::weekend_db | src/schema.rs:118:64– | 3787..3788 | try_propagation | parse | S1 |
| I | weekend-db::weekend_db | src/schema.rs:119:48– | 3837..3838 | try_propagation | parse | S3/S7 |
| P | weekend-db::weekend_db | src/schema.rs:127:9– | 3987..3989 | if | parse | S3 |
| C | weekend-db::weekend_db | src/schema.rs:127:12– | 3990..3993 | pattern_test | parse | S3 |
| E | weekend-db::weekend_db | src/schema.rs:130:55– | 4134..4135 | try_propagation | parse | S3 |
| P | weekend-db::weekend_db | src/schema.rs:135:9– | 4239..4241 | if | check | S3 |
| P | weekend-db::weekend_db | src/schema.rs:136:13– | 4283..4285 | if | check | S3 |
| P | weekend-db::weekend_db | src/schema.rs:141:16– | 4422..4424 | if | check | S3 |
| E | weekend-db::weekend_db | src/schema.rs:149:25– | 4637..4638 | try_propagation | required_value_bool | S3 |
| P | weekend-db::weekend_db | src/schema.rs:169:9– | 5073..5075 | if | build | S1/S2 |
| C | weekend-db::weekend_db | src/schema.rs:169:12– | 5076..5104 | short_circuit_or | build | S1/S2 |
| D | weekend-db::weekend_db | src/schema.rs:172:31– | 5197..5198 | try_propagation | build | S1/S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:173:9– | 5208..5211 | for_iteration | build | S1/S2 |
| P | weekend-db::weekend_db | src/schema.rs:174:13– | 5241..5243 | if | build | S1/S2 |
| C | weekend-db::weekend_db | src/schema.rs:174:16– | 5244..5303 | short_circuit_or | build | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:174:64– | 5292..5293 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:178:23– | 5427..5428 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:179:66– | 5494..5495 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:179:68– | 5496..5497 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:180:65– | 5562..5563 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:180:67– | 5564..5565 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:181:65– | 5630..5631 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:181:67– | 5632..5633 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:182:65– | 5698..5699 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:182:67– | 5700..5701 | try_propagation | build | S1/S2/S7 |
| P | weekend-db::weekend_db | src/schema.rs:183:13– | 5715..5717 | if | build | S1/S2 |
| C | weekend-db::weekend_db | src/schema.rs:183:16– | 5718..5721 | pattern_test | build | S1/S2 |
| D | weekend-db::weekend_db | src/schema.rs:184:59– | 5812..5813 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:186:59– | 5887..5888 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:188:36– | 5935..5936 | try_propagation | build | S1/S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:189:9– | 5946..5949 | for_iteration | build | S1/S2 |
| D | weekend-db::weekend_db | src/schema.rs:190:58– | 6024..6025 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:190:60– | 6026..6027 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:193:21– | 6085..6086 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:194:56– | 6142..6143 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:194:58– | 6144..6145 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:195:59– | 6204..6205 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:195:61– | 6206..6207 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:196:42– | 6249..6250 | try_propagation | build | S1/S2/S7 |
| D | weekend-db::weekend_db | src/schema.rs:197:48– | 6298..6299 | try_propagation | build | S1/S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:201:46– | 6414..6415 | try_propagation | from_value | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:202:49– | 6465..6466 | try_propagation | from_value | S1/S2 |
| P | weekend-db::weekend_db | src/schema.rs:203:9– | 6476..6478 | if | from_value | S1/S2 |
| C | weekend-db::weekend_db | src/schema.rs:203:12– | 6479..6507 | short_circuit_or | from_value | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:206:47– | 6616..6617 | try_propagation | from_value | S1/S2 |
| P | weekend-db::weekend_db | src/schema.rs:207:9– | 6627..6629 | if | from_value | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:211:51– | 6820..6821 | try_propagation | from_value | S1/S2 |
| P | weekend-db::weekend_db | src/schema.rs:212:9– | 6831..6833 | if | from_value | S1/S2 |
| E | weekend-db::weekend_db | src/schema.rs:215:9– | 6934..6937 | for_iteration | from_value | S2 |
| I | weekend-db::weekend_db | src/schema.rs:215:35– | 6960..6961 | try_propagation | from_value | S2/S7 |
| I | weekend-db::weekend_db | src/schema.rs:217:29– | 7022..7023 | try_propagation | from_value | S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:219:55– | 7104..7105 | try_propagation | from_value | S2 |
| P | weekend-db::weekend_db | src/schema.rs:220:13– | 7119..7121 | if | from_value | S2 |
| C | weekend-db::weekend_db | src/schema.rs:220:16– | 7122..7174 | short_circuit_or | from_value | S2 |
| I | weekend-db::weekend_db | src/schema.rs:220:57– | 7163..7164 | try_propagation | from_value | S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:223:13– | 7257..7260 | for_iteration | from_value | S2 |
| P | weekend-db::weekend_db | src/schema.rs:224:17– | 7289..7291 | if | from_value | S2 |
| I | weekend-db::weekend_db | src/schema.rs:224:40– | 7312..7313 | try_propagation | from_value | S2/S7 |
| I | weekend-db::weekend_db | src/schema.rs:224:50– | 7322..7323 | try_propagation | from_value | S2/S7 |
| E | weekend-db::weekend_db | src/schema.rs:229:9– | 7440..7443 | for_iteration | from_value | S2 |
| E | weekend-db::weekend_db | src/schema.rs:230:64– | 7557..7558 | try_propagation | from_value | S2 |
| E | weekend-db::weekend_db | src/schema.rs:231:52– | 7611..7612 | try_propagation | from_value | S2 |
| E | weekend-db::weekend_db | src/schema.rs:232:13– | 7626..7629 | for_iteration | from_value | S2 |
| P | weekend-db::weekend_db | src/schema.rs:233:17– | 7679..7681 | if | from_value | S2 |
| I | weekend-db::weekend_db | src/schema.rs:233:50– | 7712..7713 | try_propagation | from_value | S2/S7 |
| I | weekend-db::weekend_db | src/schema.rs:254:39– | 8218..8219 | try_propagation | field | S2/S7/S7 |
| I | weekend-db::weekend_db | src/schema.rs:255:37– | 8256..8257 | try_propagation | field | S2/S7/S7 |
| I | weekend-db::weekend_db | src/schema.rs:260:51– | 8420..8421 | try_propagation | reserved | S2/S7/S7 |
| E | weekend-db::weekend_db | src/schema.rs:261:9– | 8431..8434 | for_iteration | reserved | S2/S7 |
| I | weekend-db::weekend_db | src/schema.rs:261:30– | 8452..8453 | try_propagation | reserved | S2/S7/S7 |
| P | weekend-db::weekend_db | src/schema.rs:262:13– | 8468..8470 | if | reserved | S2/S7 |
| I | weekend-db::weekend_db | src/schema.rs:262:31– | 8486..8487 | try_propagation | reserved | S2/S7/S7 |
| I | weekend-db::weekend_db | src/schema.rs:262:41– | 8496..8497 | try_propagation | reserved | S2/S7/S7 |
| P | weekend-db::weekend_db | src/schema.rs:269:9– | 8656..8658 | if | successor_of | S4 |
| C | weekend-db::weekend_db | src/schema.rs:269:12– | 8659..8738 | short_circuit_or | successor_of | S4 |
| E | weekend-db::weekend_db | src/schema.rs:272:9– | 8804..8807 | for_iteration | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:272:62– | 8857..8858 | try_propagation | successor_of | S4/S7 |
| I | weekend-db::weekend_db | src/schema.rs:273:36– | 8917..8918 | try_propagation | successor_of | S4/S7 |
| E | weekend-db::weekend_db | src/schema.rs:274:13– | 8932..8937 | match | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:274:33– | 8952..8953 | try_propagation | successor_of | S4/S7 |
| P | weekend-db::weekend_db | src/schema.rs:275:30– | 8985..9026 | match_guard | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:275:65– | 9020..9021 | try_propagation | successor_of | S4/S7 |
| P | weekend-db::weekend_db | src/schema.rs:276:25– | 9057..9075 | match_guard | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:276:42– | 9074..9075 | try_propagation | successor_of | S4/S7 |
| I | weekend-db::weekend_db | src/schema.rs:280:55– | 9214..9215 | try_propagation | successor_of | S4/S7 |
| E | weekend-db::weekend_db | src/schema.rs:281:9– | 9225..9228 | for_iteration | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:281:35– | 9251..9252 | try_propagation | successor_of | S4/S7 |
| P | weekend-db::weekend_db | src/schema.rs:282:13– | 9267..9269 | if | successor_of | S4 |
| I | weekend-db::weekend_db | src/schema.rs:282:51– | 9305..9306 | try_propagation | successor_of | S4/S7 |
| I | weekend-db::weekend_db | src/schema.rs:282:61– | 9315..9316 | try_propagation | successor_of | S4/S7 |
| I | weekend-db::weekend_db | src/schema.rs:282:63– | 9317..9318 | try_propagation | successor_of | S4/S7 |
| E | weekend-db::weekend_db | src/schema.rs:295:71– | 9663..9664 | try_propagation | load | S4 |
| E | weekend-db::weekend_db | src/schema.rs:296:58– | 9722..9723 | try_propagation | load | S4 |
| E | weekend-db::weekend_db | src/schema.rs:297:45– | 9769..9770 | try_propagation | load | S4 |
| P | weekend-db::weekend_db | src/schema.rs:298:9– | 9780..9782 | if | load | S4 |
| C | weekend-db::weekend_db | src/schema.rs:298:12– | 9783..9837 | short_circuit_or | load | S4 |
| P | weekend-db::weekend_db | src/schema.rs:308:13– | 10289..10291 | if | publish | S5 |
| C | weekend-db::weekend_db | src/schema.rs:308:16– | 10292..10295 | pattern_test | publish | S5 |
| D | weekend-db::weekend_db | src/schema.rs:308:61– | 10337..10338 | try_propagation | publish | S5/S7 |
| P | weekend-db::weekend_db | src/schema.rs:309:24– | 10364..10366 | if | publish | S5 |
| P | weekend-db::weekend_db | src/schema.rs:315:13– | 10539..10541 | if | publish | S5 |
| E | weekend-db::weekend_db | src/schema.rs:316:90– | 10650..10651 | try_propagation | publish | S5 |
| E | weekend-db::weekend_db | src/schema.rs:317:45– | 10697..10698 | try_propagation | publish | S5 |
| D | weekend-db::weekend_db | src/schema.rs:319:48– | 10761..10762 | try_propagation | publish | S5/S7 |
| P | weekend-db::weekend_db | src/schema.rs:322:9– | 10805..10807 | if | publish | S5 |
| C | weekend-db::weekend_db | src/schema.rs:322:12– | 10808..10865 | short_circuit_and | publish | S5 |
| P | weekend-db::weekend_db | src/schema.rs:328:9– | 10995..10997 | if | envelope | S3/S6 |
| C | weekend-db::weekend_db | src/schema.rs:328:12– | 10998..11062 | short_circuit_or | envelope | S3/S6 |
| E | weekend-db::weekend_db | src/schema.rs:335:30– | 11265..11266 | try_propagation | read_field | S3/S6 |
| I | weekend-db::weekend_db | src/schema.rs:336:35– | 11302..11303 | try_propagation | read_field | S3/S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:336:67– | 11334..11335 | try_propagation | read_field | S3/S6 |
| P | weekend-db::weekend_db | src/schema.rs:337:9– | 11345..11347 | if | read_field | S3/S6 |
| C | weekend-db::weekend_db | src/schema.rs:337:12– | 11348..11351 | pattern_test | read_field | S3/S6 |
| E | weekend-db::weekend_db | src/schema.rs:337:46– | 11382..11383 | try_propagation | read_field | S3/S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:338:36– | 11421..11422 | try_propagation | read_field | S3/S6 |
| P | weekend-db::weekend_db | src/schema.rs:341:9– | 11478..11480 | if | read_field | S3/S6 |
| C | weekend-db::weekend_db | src/schema.rs:341:12– | 11481..11484 | pattern_test | read_field | S3/S6 |
| P | weekend-db::weekend_db | src/schema.rs:344:9– | 11573..11575 | if | read_field | S3/S6 |
| E | weekend-db::weekend_db | src/schema.rs:351:30– | 11777..11778 | try_propagation | validate | S3/S6 |
| E | weekend-db::weekend_db | src/schema.rs:352:9– | 11788..11791 | for_iteration | validate | S3/S6 |
| I | weekend-db::weekend_db | src/schema.rs:352:55– | 11834..11835 | try_propagation | validate | S3/S6/S7 |
| I | weekend-db::weekend_db | src/schema.rs:353:51– | 11909..11910 | try_propagation | validate | S3/S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:353:53– | 11911..11912 | try_propagation | validate | S3/S6 |
| E | weekend-db::weekend_db | src/schema.rs:355:9– | 11932..11935 | for_iteration | validate | S3/S6 |
| I | weekend-db::weekend_db | src/schema.rs:355:40– | 11963..11964 | try_propagation | validate | S3/S6/S7 |
| I | weekend-db::weekend_db | src/schema.rs:356:36– | 12023..12024 | try_propagation | validate | S3/S6/S7 |
| P | weekend-db::weekend_db | src/schema.rs:357:13– | 12038..12040 | if | validate | S3/S6 |
| I | weekend-db::weekend_db | src/schema.rs:357:33– | 12058..12059 | try_propagation | validate | S3/S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:376:21– | 12523..12524 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:377:63– | 12587..12588 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:377:65– | 12589..12590 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:378:66– | 12656..12657 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:378:68– | 12658..12659 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:379:46– | 12705..12706 | try_propagation | new | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:379:48– | 12707..12708 | try_propagation | new | S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:383:80– | 12857..12858 | try_propagation | from_value | S6 |
| E | weekend-db::weekend_db | src/schema.rs:384:83– | 12942..12943 | try_propagation | from_value | S6 |
| I | weekend-db::weekend_db | src/schema.rs:385:44– | 12988..12989 | try_propagation | from_value | S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:385:73– | 13017..13018 | try_propagation | from_value | S6 |
| P | weekend-db::weekend_db | src/schema.rs:386:9– | 13028..13030 | if | from_value | S6 |
| C | weekend-db::weekend_db | src/schema.rs:386:12– | 13031..13059 | short_circuit_or | from_value | S6 |
| C | weekend-db::weekend_db | src/schema.rs:386:12– | 13031..13089 | short_circuit_or | from_value | S6 |
| E | weekend-db::weekend_db | src/schema.rs:389:9– | 13156..13159 | for_iteration | from_value | S6 |
| E | weekend-db::weekend_db | src/schema.rs:390:27– | 13228..13229 | try_propagation | from_value | S6 |
| I | weekend-db::weekend_db | src/schema.rs:408:36– | 13597..13598 | try_propagation | fields | S6/S7 |
| P | weekend-db::weekend_db | src/schema.rs:411:9– | 13701..13703 | if | field | S6 |
| I | weekend-db::weekend_db | src/schema.rs:414:25– | 13794..13795 | try_propagation | field | S6/S7 |
| E | weekend-db::weekend_db | src/schema.rs:414:50– | 13819..13820 | try_propagation | field | S6/S7 |
| P | weekend-db::weekend_db | src/schema.rs:417:9– | 13900..13902 | if | set | S6 |
| I | weekend-db::weekend_db | src/schema.rs:422:51– | 14066..14067 | try_propagation | set | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:422:83– | 14098..14099 | try_propagation | set | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:422:85– | 14100..14101 | try_propagation | set | S6/S7 |
| P | weekend-db::weekend_db | src/schema.rs:426:9– | 14179..14181 | if | remove | S6 |
| I | weekend-db::weekend_db | src/schema.rs:431:51– | 14345..14346 | try_propagation | remove | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:431:79– | 14373..14374 | try_propagation | remove | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:431:81– | 14375..14376 | try_propagation | remove | S6/S7 |
| P | weekend-db::weekend_db | src/schema.rs:435:9– | 14482..14484 | if | with_schema | S6 |
| D | weekend-db::weekend_db | src/schema.rs:440:66– | 14685..14686 | try_propagation | with_schema | S6/S7 |
| D | weekend-db::weekend_db | src/schema.rs:440:68– | 14687..14688 | try_propagation | with_schema | S6/S7 |
