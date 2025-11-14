# sample logs

## dyeh

```plaintext
[2025-10-27 16:42:47.369] [info] effect_sdk effectsdk logtolocal add

[2025-10-27 16:42:47.867] [info] BEF::bef_effect_onResume_imp( 0x1, -1,)
EffectSDK-1870 [bef_effect_api.cpp 1399] bef_effect_onResume type: -1
EffectSDK-1870 [EffectManager.cpp 7864] onResume: EffectManager >> onResume, type = -1
BEF::bef_effect_set_effect_reload( 0x1, /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1, 1,)
BEF::bef_effect_set_sticker_with_tag_imp( 0x1, 0d, /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1, 0, 1, ,)
BEF::bef_effect_set_sticker_with_tag_v2_imp( 0x1, 0, /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1, 0, 1, ,)
EffectSDK-1870 [bef_effect_api.cpp 2887] bef_effect_set_sticker: handle=0x1, stickerId=0, stickerPath=/Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1, needReload=true, stickerTag=
EffectSDK-1870 [EffectManager.cpp 3293] stickerId = 0 reqId=0 EffectManager::setEffect::effectPath = /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1 
EffectSDK-1870 [EffectParser.cpp 411] ---EffectParser::parse: models: [  ], priority : 1
EffectSDK-1870 [EffectConfigManager.cpp 118] switchEffect set m_curEffectConfig reqId=0 strPath=/Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1/
BEF::bef_effect_load_resource_with_timeout_imp( 0x1, 10000000,)
EffectSDK-1870 [EffectABConfig.cpp 1783] EffectABConfig::getABValue : license = "", key = "enable_imageprocessor_preload_margin", value = 1
EffectSDK-1870 [BEFEffect.cpp 1046] EFFECT_RES_STATE_SUC
## 2025-10-27 16:42:47 [threadid:1954476032,AMGSceneResetController.cpp,49] ERROR ## [AE_EFFECT_TAG][SceneResetController] m_enableSceneResetController: 1
## 2025-10-27 16:42:47 [threadid:1954476032,AMGSerializedFile.cpp,726] ERROR ## [AE_GAME_TAG]object info not found for localId:0
## 2025-10-27 16:42:47 [threadid:1954476032,AMGSceneLoader.cpp,537] ERROR ## [AE_GAME_TAG]load scene end, scene path: /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1/AmazingFeature/
EffectSDK-1870 [DefaultAssetLoader.cpp 189] DefaultAssetLoader  canceled  EH_macoseufcgo8qabun1d2d0wz9eq
EffectSDK-1870 [DefaultAssetLoader.cpp 259] DefaultAssetLoader  delete  EH_macoseufcgo8qabun1d2d0wz9eq
EffectSDK-1870 [AlgorithmSensorDataDelegate.cpp 267] Sensor Disable : GravityData AccelerationData GyroData
EffectSDK-1870 [EffectManager.cpp 900] EffectManager::onSwitchEnd, effectName is EH_macoseufcgo8qabun1d2d0wz9eq/, effectPath is /Users/bytedance/Library/Application Support/DouyinAR/Instances/Instance1/Preview/previewSticker/sticker_1/, m_enableLockAlgorithmTextureCache is 0
EffectSDK-1870 [AlgorithmManager.cpp 1764] +++++ BEFEffect: AE_ALGORITHM_TAG : bALG_BACH_CONFIG tag is true in active effect's tagInfos or config.json
## 2025-10-27 16:42:47 [threadid:4013514944,AlgorithmGraphParser.cpp,151] ERROR ## [AE_ALGORITHM_TAG]BachGraphParser: parse config file: {"version": "1.0"} failed

BEF::bef_effect_onResume_imp( 0x1, -1,)
EffectSDK-1870 [bef_effect_api.cpp 1399] bef_effect_onResume type: -1
EffectSDK-1870 [EffectManager.cpp 7864] onResume: EffectManager >> onResume, type = -1
```

## ios

### raw logs

```plaintext
Oct 29 11:27:36 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:27:36.222][6923:6151464448][I][VESDK][，，89] ## 2025-10-29 11:27:36 [threadid:1892331520,ConsoleModule.cpp,103] SYSTEM ## [AE_JSRUNTIME_TAG]'bzh_IEJsEntrySystemScript: onStart registerAPJs'
Oct 29 11:27:39 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:27:39.740][6923:6151464448][E][VESDK][，，89] ## 2025-10-29 11:27:39 [threadid:1892331520,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: parsedCaption: {"text":"日常记录中","start_time":0,"end_time":3000,"words":[{"text":"日","start_time":0,"end_time":600,"is_key":false},{"text":"常","start_time":600,"end_time":1200,"is_key":false},{"text":"记","start_time":1200,"end_time":1800,"is_key":false},{"text":"录","start_time":1800,"end_time":2400,"is_key":false},{"text":"中","start_time":2400,"end_time":3000,"is_key":false}]}'
Oct 29 11:28:14 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:28:14.419][6923:4534763520*][W][VESDK][，，89] [WARN][W][VESDK][662] refresh... is refreshing
Oct 29 11:28:42 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:28:42.842][6923:4534763520*][I][VESDK][，，89] [INFO][I][VESDK][14] begin performance : refresh
Oct 29 11:28:40 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:28:40.907][6923:4534763520*][I][VESDK][，，89] [INFO][I][def][569582][VEPreviewControlImpl.cpp:838->refreshCurrentFrame][VESDK-VEPublic][0x16a880398]flag: 0
Oct 29 11:28:35 EffectCam[6923] <Notice>: [2025-10-29 +8.0 11:28:35.369][6923:4534763520*][I][VESDK][，，89] [INFO][I][VESDK][14] begin performance : refresr
Oct 29 11:28:35 EffectCam[CacheDelete](6923) <Notice>: 333 CDRecentVolumeInfo _recentInfoAtUrgency, service: com.apple.WebBookmarks.CacheDelete, amount: 0 <private>
Oct 29 11:28:35 EffectCam[CacheDelete](6923) <Notice>: com.apple.PODCAST : 0
Oct 29 11:28:37 EffectCam[UIKitCore](6923) <Notice>: Ending background task with UIBackgroundTaskIdentifier: 74r

Oct 29 11:34:10 backboardd[CoreBrightness](68) <Notice>: Transition in AOD done
Oct 29 11:36:09 backboardd[CoreBrightness](68) <Notice>: [Power Assertion] Released=0 (assertionObj=0x0)

Nov 14 15:19:24.335062 AwemeInhouse(AwemeCore)[18967] <Notice>: [2025-11-14 +8.0 15:19:24.334][18967:13311872576][E][Tool.vesdk][，，0] ## 2025-11-14 15:19:24 [threadid:1977790464,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: ieSurface created'
```

### effect logs

#### douyin style

```plaintext
Nov 14 16:25:36.045160 AwemeInhouse(AwemeCore)[19402] <Notice>: [2025-11-14 +8.0 16:25:36.044][19402:6154038208][E][Tool.vesdk][，，0] ## 2025-11-14 16:25:36 [threadid:1895936000,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onEnter being called'
```

should be parsed to

```plaintext
[threadid:1895936000,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onStart being called'
```

## android

### raw logs

```plaintext
[ 11-14 14:50:22.618  3264: 3264 I/wificond ]
station_bandwidth: 

[ 11-14 14:50:25.636  3264: 3264 I/wificond ]
station_bandwidth: 

[ 11-14 14:50:28.659  3264: 3264 I/wificond ]
station_bandwidth: 

[ 11-14 14:50:28.693  2880: 2924 I/ThermalObserver ]
Gallery hdr is disable by thermal.

[ 11-14 14:50:28.958  2880: 6202 D/Aurogon  ]
 packageName = com.google.android.gms isAllowWakeUpList 
 
[ 11-14 15:46:54.582 20387:30427 E/         ]
## 2025-11-14 15:46:54 [tid:30427,ConsoleModule.cpp:100] error ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onReload being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onReload (:39525:19)
    at onEnter (:39508:173)

[ 11-14 15:46:54.582 20387:30427 E/[Effect] ]
## 2025-11-14 15:46:54 [tid:30427,ConsoleModule.cpp:100] error ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onReload being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onReload (:39525:19)
    at onEnter (:39508:173)

[ 11-14 15:48:35.135 20387:30427 E/         ]
## 2025-11-14 15:48:35 [tid:30427,AMGRichTextParser.cpp:861] error ## [AE_TEXT_TAG]GetLetterRangeFromLetterRange, style 1953785196, 'letterRange' param invalid!

[ 11-14 15:48:35.135 20387:30427 E/[Effect] ]
## 2025-11-14 15:48:35 [tid:30427,AMGRichTextParser.cpp:861] error ## [AE_TEXT_TAG]GetLetterRangeFromLetterRange, style 1953785196, 'letterRange' param invalid!

[ 11-14 15:48:35.131 20387:30427 I/[Effect] ]
## 2025-11-14 15:48:35 [tid:30427,AMGText.cpp:885] info ## [AE_TEXT_TAG]Set Text bloom path: 

[ 11-14 15:53:49.156 20387:12953 V/unknown:c ]
Prepared frame frame 15.

[ 11-14 15:59:08.632 22894:23431 I/NativeImage ]
[WxImageLoader] Invoke Listener finished, done.

[ 11-14 15:59:08.758  2880: 5525 D/MiuiNetworkPolicy ]
updateLimit mLimitEnabled=true,enabled=false,mNetworkPriorityMode=1,mThermalForceMode=0

[ 11-14 15:59:09.022  3331: 3331 I/cnss-daemon ]
nl80211 response handler invoked
```

### effect logs

#### douyin style

- allowed:

```plaintext
[ 11-14 16:07:49.834 20387:30427 E/[Effect] ]
## 2025-11-14 16:07:49 [tid:30427,ConsoleModule.cpp:100] error ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onStart being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onStart (:39501:19)
```

should be parsed to:

```plaintext
[tid:1066,ConsoleModule.cpp:100] error ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onStart being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onStart (:39501:19)
```

disallowed:

```plaintext
[ 11-14 16:07:49.888 20387:30427 E/         ]
## 2025-11-14 16:07:49 [tid:30427,ConsoleModule.cpp:100] error ## [AE_JSRUNTIME_TAG]'sg_script_dbg: ieSurface created'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at getSurfaceProcessor (:39502:615)
    at onStart (:39501:72)
```

notice that allowed logs contain [Effect] as a tag, while not allowed logs do not. we disallow logs without that tag because they are duplicates.

ame style:

allowed:

```plaintext
[ 11-14 16:09:51.022 26992:27809 E/CKE-Editor ]
[VESDK]## 2025-11-14 16:09:51 [threadid:4161787696,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onStart being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/04f0dced960105ba3ed33740f3b71ade/SurfaceGraphScriptLoop_1762939532350.js:40:57)
    at onStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:58133)
    at checkLoadEnterStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:63545)
    at seekAnimations (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:113396)
    at onUpdate (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:278097)
    at <anonymous> (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:324826)
    at forEach (native)
    at onUpdate (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:324829)
```

disallowed:

```plaintext
[ 11-14 16:09:51.021 26992:27809 E/AE_JSRUNTIME_TAG ]
## 2025-11-14 16:09:51 [threadid:4161787696,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'sg_script_dbg: onStart being called'
    at value (file:///bootstrap/bootstrap.js:3:2225)
    at onStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/04f0dced960105ba3ed33740f3b71ade/SurfaceGraphScriptLoop_1762939532350.js:40:57)
    at onStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:58133)
    at checkLoadEnterStart (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:63545)
    at seekAnimations (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:113396)
    at onUpdate (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:278097)
    at <anonymous> (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:324826)
    at forEach (native)
    at onUpdate (file:////data/user/0/com.ss.android.ies.ugc.cam/files/effect/b6c6564c13f7ce2782ad71f0d215d13c/js/main.js:2:324829)
```

notice that allowed logs contain CKE-Editor as a tag, while not allowed logs do not. we disallow logs without that tag because they are duplicates.
