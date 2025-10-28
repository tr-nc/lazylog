# Rules for CLAUDE

You should ONLY work on the item(s) that is marked with TODO, and nothing more.

## new log format

Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: Check modules update for ALS: <private>
Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: CB features update for Collected modules info
 <private>
Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: WP matrix from state = <private>
Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: WP update = 0    delta uv = 0.000000   current (0.347640;0.354918) CCT = 4919.000000 -> target (0.347640;0.354918) CCT = 4919.000000
Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: Transition in AOD done
Oct 28 19:24:46 backboardd[CoreBrightness](68) <Notice>: [Power Assertion] Released=0 (assertionObj=0x0)
Oct 28 19:24:47 kernel()[0] <Notice>: wlan0:com.apple.p2p: isInfraRealtimePacketThresholdAllowed allowed:1 option:32 threshold:50 noRegistrations:1 cachedPeerCount:0 fastDiscoveryInactive:1 fastDiscoveryOnSince:663943308
Oct 28 19:24:47 kernel()[0] <Notice>: wlan0:com.apple.p2p: currentInfraTrafficType:8981 checking if realtime upgrade required with inputPackets:0 outputPackets:0 packetThreshold:50
Oct 28 19:24:47 dasd[80] <Notice>: Checking control action

## your goal

format the original log format into:

time: use the framework, discard the stuff: Oct 28 19:24:46
tag: the next item, backboardd[CoreBrightness](68), process it into backboardd, kernel()[0], process it into kernel, the idea is to only leave the name, before [ or (
level: the next item, <Notice>, process it into Notice, if it is <Error>, process it into Error ...
