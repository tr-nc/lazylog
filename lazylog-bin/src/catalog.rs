use std::io;

pub(crate) const TARGET_APPS: [TargetApp; 2] = [TargetApp::EffectCam, TargetApp::Douyin];
pub(crate) const PROVIDERS: [ProviderKind; 4] = [
    ProviderKind::Ios,
    ProviderKind::Android,
    ProviderKind::DyehPreview,
    ProviderKind::DyehEditor,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetApp {
    EffectCam,
    Douyin,
}

impl TargetApp {
    pub(crate) const EFFECTCAM_IOS_BUNDLE_ID: &'static str = "com.ss.ios.ugc.EffectCamInhouse";
    pub(crate) const DOUYIN_IOS_BUNDLE_ID: &'static str = "com.ss.iphone.ugc.AwemeInhouse";
    pub(crate) const EFFECTCAM_ANDROID_PACKAGE: &'static str = "com.ss.android.ies.ugc.cam";
    pub(crate) const DOUYIN_ANDROID_PACKAGE: &'static str = "com.ss.android.ugc.aweme";

    pub(crate) fn parse(value: &str) -> Result<Self, io::Error> {
        match value.to_ascii_lowercase().as_str() {
            "effectcam" | "xiangsu" => Ok(Self::EffectCam),
            "douyin" | "aweme" => Ok(Self::Douyin),
            _ if value == "像塑" || value == "像塑内测版" => Ok(Self::EffectCam),
            _ if value == "抖音" => Ok(Self::Douyin),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("Unknown app '{value}'; expected effectcam or douyin"),
            )),
        }
    }

    pub(crate) fn ios_bundle_id(self) -> &'static str {
        match self {
            Self::EffectCam => Self::EFFECTCAM_IOS_BUNDLE_ID,
            Self::Douyin => Self::DOUYIN_IOS_BUNDLE_ID,
        }
    }

    pub(crate) fn android_package(self) -> &'static str {
        match self {
            Self::EffectCam => Self::EFFECTCAM_ANDROID_PACKAGE,
            Self::Douyin => Self::DOUYIN_ANDROID_PACKAGE,
        }
    }

    pub(crate) fn alias(self) -> &'static str {
        match self {
            Self::EffectCam => "effectcam",
            Self::Douyin => "douyin",
        }
    }

    pub(crate) fn ios_display_name(self) -> &'static str {
        match self {
            Self::EffectCam => "像塑内测版",
            Self::Douyin => "抖音开发版",
        }
    }

    pub(crate) fn android_display_name(self) -> &'static str {
        match self {
            Self::EffectCam => "像塑",
            Self::Douyin => "抖音",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Ios,
    Android,
    DyehPreview,
    DyehEditor,
}

impl ProviderKind {
    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Ios => "iOS",
            Self::Android => "Android",
            Self::DyehPreview => "DYEH 预览",
            Self::DyehEditor => "DYEH 编辑器",
        }
    }

    pub(crate) fn mode_name(self) -> &'static str {
        match self {
            Self::Ios => "ios",
            Self::Android => "android",
            Self::DyehPreview => "dyeh preview",
            Self::DyehEditor => "dyeh editor",
        }
    }
}
