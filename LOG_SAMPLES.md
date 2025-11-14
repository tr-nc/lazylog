
## sample logs

- dyeh

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

- ios

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
```

- android

```plaintext
11-12 18:13:06.542  2006  4807 I SDM     : HWCSession::ProcessCWBStatus: CWB queue is empty. display_index: 0
11-12 18:13:06.543  3525  4761 D libsensor-parseRGB: the value of RGB [1346.480926 1450.355615 1017.965135]
11-12 18:13:06.624  2873  5893 D SLM-SRV-SLAService: checktemp temperature = 34806 temperature_average = 34008 isPerformanceMode = false thermal_enable_slm = true
11-12 18:13:06.686  5628  5628 D ControlCenterHeaderExpandController: onExpansionChanged: progress =  0.0
11-12 18:13:06.691  5628  5628 D ControlCenterHeaderExpandController: onExpansionChanged: progress =  0.0
11-12 18:13:06.693  5628  5628 D ControlCenterHeaderExpandController: onExpansionChanged: progress =  0.0
11-12 18:13:06.726  2006  2739 I vendor.qti.hardware.display.composer-service: FrameNotifyProcess Sensor: notify citsensorservice to trigger cwb
11-12 18:13:06.727  3525  4760 I libsensor-parseRGB: request dump start for mRequestDisplayId 0
11-12 18:13:06.727  3525  4760 D vendor.xiaomi.sensor.citsensorservice@2.0-service: handle id:10 wxh:1280x2400 uwxuh:1080x2400 size: 9216000 fd:17 fd_meta:18 flags:0x228 usage:0x33  format:0x3 layer_count: 1 reserved_size = 0
11-12 18:13:06.728  2006  2697 W SDM     : HWCSession::GetDisplayIndex: Display index not found for display 1.
11-12 18:13:06.728  2006  2697 W SDM     : HWCSession::GetDisplayIndex: Display index not found for display 3.
11-12 18:13:06.728  2006  2697 D SDM     : HWCSession::SetCWBOutputBuffer: CWB config passed by cwb_client : tappoint 1  CWB_ROI : (591.000000 22.000000 717.000000 148.000000). Display 0
11-14 14:58:08.422 15711 16559 E AE_JSRUNTIME_TAG: ## 2025-11-14 14:58:08 [threadid:4004026160,ConsoleModule.cpp,100] ERROR ## [AE_JSRUNTIME_TAG]'[AmazingProRuntime] onUpdate'
```
