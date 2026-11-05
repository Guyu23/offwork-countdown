// 下班倒计时桌面挂件
// 目标框架：.NET Framework 4.8（Windows 10/11 自带，无需另装运行时）
using System;
using System.ComponentModel;
using System.Globalization;
using System.IO;
using System.Media;
using System.Runtime.InteropServices;
using System.Runtime.Serialization;
using System.Runtime.Serialization.Json;
using System.Text;
using System.Threading;
using System.Windows;
using System.Windows.Automation;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Documents;
using System.Windows.Input;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Animation;
using System.Windows.Media.Effects;
using System.Windows.Media.Imaging;
using System.Windows.Shapes;
using System.Windows.Threading;
using Drawing = System.Drawing;
using Drawing2D = System.Drawing.Drawing2D;
using IOPath = System.IO.Path;
using WinForms = System.Windows.Forms;

[assembly: System.Reflection.AssemblyTitle("下班倒计时")]
[assembly: System.Reflection.AssemblyDescription("常驻桌面右上角的下班倒计时挂件")]
[assembly: System.Reflection.AssemblyProduct("下班倒计时")]
[assembly: System.Reflection.AssemblyVersion("1.1.0.0")]
[assembly: System.Reflection.AssemblyFileVersion("1.1.0.0")]

namespace OffWorkCountdown
{
    [DataContract]
    internal sealed class CountdownSettings
    {
        public const string DefaultStart = "09:00";
        public const string DefaultEnd = "18:30";
        public const string DefaultLanguage = "zh";

        [DataMember(Name = "startTime")]
        public string StartTime { get; set; }

        [DataMember(Name = "endTime")]
        public string EndTime { get; set; }

        [DataMember(Name = "language", EmitDefaultValue = false)]
        public string Language { get; set; }

        [DataMember(Name = "soundEnabled", EmitDefaultValue = false)]
        public bool SoundEnabled { get; set; }

        [DataMember(Name = "background", EmitDefaultValue = false)]
        public string Background { get; set; }
        [DataMember(Name = "windowRight", EmitDefaultValue = false)]
        public double? WindowRight { get; set; }

        [DataMember(Name = "windowTop", EmitDefaultValue = false)]
        public double? WindowTop { get; set; }

        public CountdownSettings()
        {
            StartTime = DefaultStart;
            EndTime = DefaultEnd;
            Language = DefaultLanguage;
            Background = "aurora";
            SoundEnabled = false;
        }

        public static CountdownSettings CreateDefault()
        {
            return new CountdownSettings();
        }

        public bool IsValid()
        {
            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            return TryNormalizeTime(StartTime, out startText, out start) &&
                   TryNormalizeTime(EndTime, out endText, out end) &&
                   end > start;
        }

        /// <summary>把用户输入统一成 HH:mm；支持 9:00 这种写法。</summary>
        public static bool TryNormalizeTime(string raw, out string normalized, out TimeSpan time)
        {
            normalized = null;
            time = TimeSpan.Zero;
            if (string.IsNullOrEmpty(raw)) return false;

            string text = raw.Trim().Replace('：', ':');
            DateTime value;
            string[] formats = new string[] { "H:mm", "HH:mm" };
            if (!DateTime.TryParseExact(
                    text,
                    formats,
                    CultureInfo.InvariantCulture,
                    DateTimeStyles.None,
                    out value))
            {
                return false;
            }

            normalized = value.ToString("HH:mm", CultureInfo.InvariantCulture);
            time = value.TimeOfDay;
            return true;
        }

        /// <summary>
        /// 更宽松的时间解析：9、900、930、1830、18:30、9:30 都接受。
        /// </summary>
        public static bool TryParseFlexibleTime(
            string raw,
            out string normalized,
            out TimeSpan time)
        {
            normalized = null;
            time = TimeSpan.Zero;
            if (string.IsNullOrEmpty(raw)) return false;

            string text = raw.Trim().Replace('：', ':');
            string hourText;
            string minuteText;

            if (text.IndexOf(':') >= 0)
            {
                string[] parts = text.Split(':');
                if (parts.Length != 2) return false;
                hourText = parts[0].Trim();
                minuteText = parts[1].Trim();
                if (minuteText.Length == 0) minuteText = "0";
            }
            else
            {
                for (int i = 0; i < text.Length; i++)
                {
                    if (!char.IsDigit(text[i])) return false;
                }
                if (text.Length == 0 || text.Length > 4) return false;
                if (text.Length <= 2)
                {
                    hourText = text;
                    minuteText = "0";
                }
                else if (text.Length == 3)
                {
                    hourText = text.Substring(0, 1);
                    minuteText = text.Substring(1);
                }
                else
                {
                    hourText = text.Substring(0, 2);
                    minuteText = text.Substring(2);
                }
            }

            int hour;
            int minute;
            if (!int.TryParse(hourText, NumberStyles.None, CultureInfo.InvariantCulture, out hour) ||
                !int.TryParse(minuteText, NumberStyles.None, CultureInfo.InvariantCulture, out minute))
            {
                return false;
            }
            if (hour < 0 || hour > 23 || minute < 0 || minute > 59) return false;

            normalized = string.Format(
                CultureInfo.InvariantCulture,
                "{0:00}:{1:00}",
                hour,
                minute);
            time = new TimeSpan(hour, minute, 0);
            return true;
        }
    }

    internal static class SettingsStore
    {
        private static readonly DataContractJsonSerializer Serializer =
            new DataContractJsonSerializer(typeof(CountdownSettings));

        private static string DirectoryPath
        {
            get
            {
                return IOPath.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    "OffWorkCountdown");
            }
        }

        public static string FilePath
        {
            get { return IOPath.Combine(DirectoryPath, "settings.json"); }
        }

        public static CountdownSettings Load()
        {
            try
            {
                if (!File.Exists(FilePath)) return CountdownSettings.CreateDefault();
                using (FileStream stream = File.OpenRead(FilePath))
                {
                    CountdownSettings saved = Serializer.ReadObject(stream) as CountdownSettings;
                    if (saved != null && saved.IsValid()) return saved;
                }
            }
            catch
            {
                // 配置损坏时回退默认值，挂件仍能启动
            }
            return CountdownSettings.CreateDefault();
        }

        public static void Save(CountdownSettings settings)
        {
            try
            {
                Directory.CreateDirectory(DirectoryPath);
                using (FileStream stream = File.Create(FilePath))
                {
                    Serializer.WriteObject(stream, settings);
                }
            }
            catch
            {
                // 配置写失败时保持当前会话可用
            }
        }
    }

    internal sealed class CountdownWindow : Window
    {
        private const string AppTitle = "下班倒计时";
        private const double ExpandedWidth = 300.0;
        private const double CollapsedWidth = 200.0;
        private const double WindowMargin = 24.0;
        private const double SettingsErrorAllowance = 24.0;
        private const double DockThreshold = 10.0;
        private const double DockSliver = 6.0;
        private const double DockSlideMs = 170.0;
        private const double DockHideDelayMs = 700.0;
        private const string AutoStartValueName = "OffWorkCountdown";
        private const string AutoStartKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

        private static readonly SolidColorBrush TextPrimary = MakeBrush(0x1B, 0x1F, 0x27);
        private static readonly SolidColorBrush TextSecondary = MakeBrush(0x5F, 0x6B, 0x7A);
        private static readonly SolidColorBrush TextDisabled = MakeBrush(0xA5, 0xAD, 0xB8);
        private static readonly SolidColorBrush AuroraTextBright = MakeBrush(0xF5, 0xFA, 0xFF);
        private static readonly SolidColorBrush AuroraTextSoft = MakeBrush(0xD6, 0xE2, 0xF0);
        private static readonly SolidColorBrush AuroraDone = MakeBrush(0x8C, 0xF0, 0xB2);
        private static readonly SolidColorBrush BorderLine = MakeBrush(0xE1, 0xE7, 0xEF);
        private static readonly SolidColorBrush Surface = MakeBrush(0xFF, 0xFF, 0xFF);
        private static readonly SolidColorBrush SurfaceMuted = MakeBrush(0xF6, 0xF8, 0xFB);
        private static readonly SolidColorBrush Track = MakeBrush(0xE8, 0xED, 0xF4);
        private static readonly SolidColorBrush StepperFill = MakeBrush(0xEA, 0xEF, 0xF6);
        private static readonly SolidColorBrush StepperLine = MakeBrush(0xD5, 0xDE, 0xE9);
        private static readonly SolidColorBrush OvernightBadgeFill = MakeBrush(0xE8, 0xF1, 0xFB);
        private static readonly SolidColorBrush Brand = MakeBrush(0x0F, 0x6C, 0xBD);
        private static readonly SolidColorBrush BrandHover = MakeBrush(0x11, 0x5E, 0xA3);
        private static readonly SolidColorBrush BrandPressed = MakeBrush(0x0C, 0x4A, 0x8A);
        private static readonly SolidColorBrush Success = MakeBrush(0x16, 0xA3, 0x4A);
        private static readonly SolidColorBrush Danger = MakeBrush(0xDC, 0x26, 0x26);
        private static readonly SolidColorBrush HoverFill = MakeBrush(0xF2, 0xF6, 0xFB);
        private static readonly SolidColorBrush PressedFill = MakeBrush(0xE6, 0xEE, 0xF7);
        private static readonly Brush CardFill = MakeCardFill();
        private static readonly Brush CardBorder = MakeCardBorder();
        private static readonly Brush ProgressFill = MakeProgressFill();
        private static readonly Brush SheenFill = MakeSheenFill();
        private static readonly Brush SpecularFill = MakeSpecularFill();

        private readonly CountdownSettings settings;
        private readonly DispatcherTimer timer;

        private Grid surface;
        private Grid contentHost;
        private AuroraEffect auroraEffect;
        private Rectangle auroraRect;
        private bool auroraActive = true;
        private OceanEffect oceanEffect;
        private Rectangle oceanRect;
        private string backdropMode = "aurora";
        private double auroraPhase;
        private double auroraLast = -1.0;
        private Border shadow;
        private Border nearShadow;
        private Rectangle sheenRect;
        private Rectangle specularRect;
        private Border card;
        private StackPanel countdownView;
        private StackPanel settingsView;
        private Button collapsedView;
        private ContentControl headerIcon;
        private ContentControl collapsedIcon;
        private TextBlock statusLabel;
        private TextBlock countdownText;
        private TextBlock doneText;
        private TextBlock footerLeftText;
        private Run footerCaptionRun;
        private Run footerTimeRun;
        private TextBlock footerRightText;
        private TextBlock collapsedLabel;
        private TextBlock collapsedTime;
        private TextBlock settingsError;
        private Grid progressTrack;
        private Border progressFill;
        private TextBox startBox;
        private TextBox endBox;
        private TextBlock backgroundLabel;
        private Button backgroundNoneButton;
        private Button backgroundAuroraButton;
        private Button backgroundOceanButton;
        private Border backgroundThumb;

        private Drawing.Icon trayDrawingIcon;
        private IntPtr trayIconHandle = IntPtr.Zero;
        private WinForms.NotifyIcon tray;
        private WinForms.ToolStripMenuItem trayToggleItem;

        private TimeSpan workStart;
        private TimeSpan workEnd;
        private FrameworkElement currentView;
        private double countdownCardHeight;
        private double settingsCardHeight;
        private double collapsedCardHeight;
        private bool anchorReady;
        private bool hasCustomPosition;
        private double anchorRight;
        private double anchorTop;
        private bool dragCandidate;
        private bool dragInProgress;
        private bool dragFromCollapsed;
        private Point dragStartPoint;
        private bool settingsOpen;
        private bool allowClose;
        private bool? lastWorking;
        private int currentPercent = 100;

        private enum DockEdge
        {
            None,
            Left,
            Right,
            Top,
            Bottom
        }

        private DockEdge dockEdge = DockEdge.None;
        private bool dockedHidden;
        private DispatcherTimer dockSlideTimer;
        private DispatcherTimer dockHideTimer;
        private DateTime dockSlideStarted;
        private double dockSlideFromLeft;
        private double dockSlideFromTop;
        private double dockSlideToLeft;
        private double dockSlideToTop;

        private CheckBox soundToggle;
        private CheckBox autoStartToggle;
        private bool suppressSettingsApply;
        private string language = "zh";

        private Button settingsButton;
        private Button collapseButton;
        private Button closeSettingsButton;
        private Button languageZhButton;
        private Button languageEnButton;
        private Border languageThumb;
        private TextBlock settingsTitle;
        private TextBlock startFieldLabel;
        private TextBlock endFieldLabel;
        private Border endFieldBadge;
        private TextBlock endFieldBadgeText;
        private TextBlock soundLabel;
        private TextBlock autoStartLabel;
        private TextBlock languageLabel;
        private MenuItem contextSettingsItem;
        private MenuItem contextHideItem;
        private MenuItem contextExitItem;
        private WinForms.ToolStripMenuItem traySettingsItem;
        private WinForms.ToolStripMenuItem trayExitItem;
        private DateTime? offWorkRevealUntil;
        private DateTime lastReminderEnd = DateTime.MinValue;
        private SoundPlayer chimePlayer;
        private Stream chimeStream;

        public CountdownWindow()
        {
            settings = SettingsStore.Load();
            language = settings.Language == "en" ? "en" : "zh";
            backdropMode = NormalizeBackdrop(settings.Background);
            settings.Background = backdropMode;
            RefreshTimeSpans();
            RefreshAutoStartPath();

            Title = AppTitle;
            WindowStyle = WindowStyle.None;
            AllowsTransparency = true;
            Background = Brushes.Transparent;
            ResizeMode = ResizeMode.NoResize;
            SizeToContent = SizeToContent.Manual;
            WindowStartupLocation = WindowStartupLocation.Manual;
            ShowInTaskbar = false;
            Topmost = true;
            ShowActivated = false;
            UseLayoutRounding = true;
            SnapsToDevicePixels = true;
            FontFamily = IsEnglish ? PickEnglishFont() : PickChineseFont();
            TextOptions.SetTextFormattingMode(this, TextFormattingMode.Display);

            Content = BuildRoot();
            MeasureCardHeights();
            surface.Width = ExpandedWidth;
            surface.Height = countdownCardHeight;
            // 顶层窗口固定为最大状态，切换卡片时不再调整窗口尺寸，
            // 避免窗口和内容之间差一帧导致的文字跳动或裁切。
            double maxCardHeight = Math.Max(
                countdownCardHeight,
                Math.Max(settingsCardHeight, collapsedCardHeight));
            Width = ExpandedWidth + WindowMargin * 2;
            Height = maxCardHeight + WindowMargin * 2 + SettingsErrorAllowance;

            InitializeAnchor();
            anchorReady = true;
            ApplyAnchor();

            SetupTray();
            if (trayIconHandle != IntPtr.Zero)
            {
                Icon = Imaging.CreateBitmapSourceFromHIcon(
                    trayIconHandle,
                    Int32Rect.Empty,
                    BitmapSizeOptions.FromEmptyOptions());
            }

            Loaded += OnLoaded;
            SizeChanged += OnSizeChanged;
            Closing += OnClosing;
            PreviewKeyDown += OnPreviewKeyDown;
            Microsoft.Win32.SystemEvents.DisplaySettingsChanged += OnDisplaySettingsChanged;

            timer = new DispatcherTimer(DispatcherPriority.Background);
            timer.Interval = TimeSpan.FromMilliseconds(500);
            timer.Tick += OnTick;
            timer.Start();

            dockSlideTimer = new DispatcherTimer(DispatcherPriority.Render);
            dockSlideTimer.Interval = TimeSpan.FromMilliseconds(15);
            dockSlideTimer.Tick += OnDockSlideTick;

            dockHideTimer = new DispatcherTimer(DispatcherPriority.Background);
            dockHideTimer.Interval = TimeSpan.FromMilliseconds(DockHideDelayMs);
            dockHideTimer.Tick += OnDockHideTick;
        }

        protected override void OnSourceInitialized(EventArgs e)
        {
            base.OnSourceInitialized(e);
            IntPtr handle = new WindowInteropHelper(this).Handle;
            int style = NativeMethods.GetWindowLong(handle, NativeMethods.GWL_EXSTYLE);
            NativeMethods.SetWindowLong(
                handle,
                NativeMethods.GWL_EXSTYLE,
                style | NativeMethods.WS_EX_TOOLWINDOW);

            HwndSource source = HwndSource.FromHwnd(handle);
            if (source != null) source.AddHook(WndProc);
        }

        private IntPtr WndProc(
            IntPtr hwnd,
            int message,
            IntPtr wParam,
            IntPtr lParam,
            ref bool handled)
        {
            if (NativeMethods.ShowWidgetMessage != 0 &&
                message == NativeMethods.ShowWidgetMessage)
            {
                ShowWidget();
                Activate();
                handled = true;
            }
            return IntPtr.Zero;
        }

        private void OnLoaded(object sender, RoutedEventArgs e)
        {
            ClampAnchor();
            ApplyAnchor();
            RefreshUi();
            UpdateTrayToggleText();
            if ((auroraEffect != null || oceanEffect != null) && (AuroraEffect.IsAvailable || OceanEffect.IsAvailable) && SystemParameters.ClientAreaAnimation)
            {
                auroraLast = -1.0;
                CompositionTarget.Rendering += OnAuroraFrame;
            }
            IsVisibleChanged += OnAuroraVisibilityChanged;
        }

        private void OnSizeChanged(object sender, SizeChangedEventArgs e)
        {
            if (!IsLoaded) return;
            if (anchorReady) ApplyAnchor();
            UpdateProgressWidth();
        }

        private void OnAuroraFrame(object sender, EventArgs e)
        {
            if (!auroraActive || !IsVisible) return;
            RenderingEventArgs args = e as RenderingEventArgs;
            if (args == null) return;
            double now = args.RenderingTime.TotalSeconds;
            if (auroraLast < 0.0)
            {
                auroraLast = now;
                return;
            }
            double delta = now - auroraLast;
            if (delta < 1.0 / 24.0 - 0.002) return;
            if (delta > 0.12) delta = 0.12;
            auroraLast = now;
            auroraPhase += delta;
            if (backdropMode == "ocean" && oceanEffect != null) oceanEffect.Time = auroraPhase;
            else if (auroraEffect != null) auroraEffect.Time = auroraPhase;
        }

        private void OnAuroraVisibilityChanged(object sender, DependencyPropertyChangedEventArgs e)
        {
            auroraLast = -1.0;
        }

        private static string NormalizeBackdrop(string mode)
        {
            if (mode == "none") return "none";
            if (mode == "ocean") return OceanEffect.IsAvailable ? "ocean" : (AuroraEffect.IsAvailable ? "aurora" : "none");
            if (mode == "aurora") return AuroraEffect.IsAvailable ? "aurora" : (OceanEffect.IsAvailable ? "ocean" : "none");
            return AuroraEffect.IsAvailable ? "aurora" : (OceanEffect.IsAvailable ? "ocean" : "none");
        }

        private void ApplyBackdropTheme()
        {
            bool dark = backdropMode != "none";
            SolidColorBrush darkCard = new SolidColorBrush(Color.FromRgb(6, 12, 20));
            darkCard.Freeze();
            if (card != null)
            {
                if (dark)
                {
                    card.Background = darkCard;
                    card.BorderBrush = null;
                    card.BorderThickness = new Thickness(0);
                }
                else
                {
                    card.Background = CardFill;
                    card.BorderBrush = CardBorder;
                    card.BorderThickness = new Thickness(1);
                }
            }
            if (shadow != null) shadow.Background = dark ? (Brush)darkCard : Surface;
            if (nearShadow != null) nearShadow.Background = dark ? (Brush)darkCard : Surface;
            if (sheenRect != null) sheenRect.Visibility = dark ? Visibility.Collapsed : Visibility.Visible;
            if (specularRect != null) specularRect.Visibility = dark ? Visibility.Collapsed : Visibility.Visible;
            if (dark)
            {
                ApplyAuroraTheme();
                ApplyAuroraSettingsTheme();
                ApplyAuroraCollapsedTheme();
            }
            else
            {
                ApplyLightTheme();
            }
            UpdateBackdropSegments();
        }

        private void ApplyLightTheme()
        {
            bool workingNow = !lastWorking.HasValue || lastWorking.Value;
            Brush stateBrush = workingNow ? (Brush)TextSecondary : Success;

            if (statusLabel != null)
            {
                statusLabel.Foreground = stateBrush;
                statusLabel.FontSize = 13.0;
                statusLabel.FontWeight = FontWeights.Normal;
                statusLabel.Effect = null;
            }
            if (headerIcon != null)
            {
                headerIcon.Width = 16.0;
                headerIcon.Height = 16.0;
                headerIcon.Effect = null;
                headerIcon.Content = workingNow ? (object)CreateClockIcon(16, TextSecondary) : (object)CreateCheckIcon(16, Success);
            }
            if (settingsButton != null) settingsButton.Content = CreateSettingsIcon(TextSecondary);
            if (collapseButton != null) collapseButton.Content = CreateMinusIcon(16, TextSecondary);
            if (closeSettingsButton != null) closeSettingsButton.Content = CreateCloseIcon(16, TextSecondary);
            if (countdownText != null)
            {
                countdownText.Foreground = TextPrimary;
                countdownText.Effect = null;
            }
            if (doneText != null)
            {
                doneText.Foreground = Success;
                doneText.Effect = null;
            }
            if (footerLeftText != null)
            {
                footerLeftText.Foreground = TextSecondary;
                footerLeftText.Effect = null;
            }
            if (footerRightText != null)
            {
                footerRightText.Foreground = TextSecondary;
                footerRightText.Effect = null;
            }
            if (progressTrack != null && progressTrack.Children.Count > 0)
            {
                Border track = progressTrack.Children[0] as Border;
                if (track != null) track.Background = Track;
            }
            if (settingsTitle != null)
            {
                settingsTitle.Foreground = TextPrimary;
                settingsTitle.Effect = null;
            }
            TextBlock[] labels = new TextBlock[] {
                startFieldLabel, endFieldLabel, soundLabel, autoStartLabel, languageLabel, backgroundLabel
            };
            for (int i = 0; i < labels.Length; i++)
            {
                if (labels[i] == null) continue;
                labels[i].Foreground = TextSecondary;
                labels[i].Effect = null;
            }
            if (settingsError != null)
            {
                settingsError.Foreground = TextDisabled;
                settingsError.Effect = null;
            }
            if (collapsedIcon != null)
            {
                collapsedIcon.Width = 16.0;
                collapsedIcon.Height = 16.0;
                collapsedIcon.Effect = null;
                collapsedIcon.Content = workingNow ? (object)CreateClockIcon(16, TextSecondary) : (object)CreateCheckIcon(16, Success);
            }
            if (collapsedLabel != null)
            {
                collapsedLabel.Foreground = stateBrush;
                collapsedLabel.FontSize = 12.5;
                collapsedLabel.Effect = null;
            }
            if (collapsedTime != null)
            {
                collapsedTime.Foreground = workingNow ? (Brush)TextPrimary : Success;
                collapsedTime.FontSize = 15.0;
                collapsedTime.Effect = null;
            }
        }

        private void ApplyAuroraTheme()
        {
            SolidColorBrush soft = new SolidColorBrush(Color.FromRgb(226, 235, 245));
            soft.Freeze();
            SolidColorBrush bright = new SolidColorBrush(Color.FromRgb(248, 251, 255));
            bright.Freeze();
            SolidColorBrush done = new SolidColorBrush(Color.FromRgb(140, 240, 178));
            done.Freeze();

            if (statusLabel != null)
            {
                statusLabel.Foreground = Brushes.White;
                statusLabel.FontSize = 14.0;
                statusLabel.FontWeight = FontWeights.SemiBold;
            }
            bool workingNow = !lastWorking.HasValue || lastWorking.Value;
            if (headerIcon != null)
            {
                headerIcon.Width = 18.0;
                headerIcon.Height = 18.0;
                headerIcon.Effect = null;
                headerIcon.Content = workingNow ? (object)CreateClockIcon(18, Brushes.White) : (object)CreateCheckIcon(18, AuroraDone);
            }
            if (settingsButton != null) settingsButton.Content = CreateSettingsIcon(Brushes.White);
            if (collapseButton != null) collapseButton.Content = CreateMinusIcon(16, Brushes.White);

            if (countdownText != null)
            {
                countdownText.Foreground = bright;
            }
            if (doneText != null)
            {
                doneText.Foreground = done;
            }
            if (footerLeftText != null)
            {
                footerLeftText.Foreground = soft;
            }
            if (footerRightText != null)
            {
                footerRightText.Foreground = soft;
            }
            if (progressTrack != null && progressTrack.Children.Count > 0)
            {
                Border track = progressTrack.Children[0] as Border;
                if (track != null)
                {
                    SolidColorBrush trackBrush = new SolidColorBrush(Color.FromArgb(80, 235, 244, 255));
                    trackBrush.Freeze();
                    track.Background = trackBrush;
                }
            }
        }

        private static DropShadowEffect CreateHeaderShadow()
        {
            DropShadowEffect shadow = new DropShadowEffect();
            shadow.Color = Colors.Black;
            shadow.BlurRadius = 4.0;
            shadow.ShadowDepth = 1.0;
            shadow.Direction = 270.0;
            shadow.Opacity = 0.88;
            shadow.RenderingBias = RenderingBias.Performance;
            shadow.Freeze();
            return shadow;
        }

        private static DropShadowEffect CreateTextShadow()
        {
            DropShadowEffect shadow = new DropShadowEffect();
            shadow.Color = Colors.Black;
            shadow.BlurRadius = 8.0;
            shadow.ShadowDepth = 1.0;
            shadow.Direction = 270.0;
            shadow.Opacity = 0.68;
            shadow.RenderingBias = RenderingBias.Performance;
            shadow.Freeze();
            return shadow;
        }

        private void SetAuroraVisible(bool visible)
        {
            if (auroraRect == null) return;
            auroraActive = visible && IsVisible;
            double target = visible ? 1.0 : 0.0;
            if (!SystemParameters.ClientAreaAnimation)
            {
                auroraRect.BeginAnimation(UIElement.OpacityProperty, null);
                auroraRect.Opacity = target;
                auroraLast = -1.0;
                return;
            }
            DoubleAnimation fade = new DoubleAnimation(
                auroraRect.Opacity,
                target,
                TimeSpan.FromMilliseconds(240));
            fade.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            auroraRect.BeginAnimation(UIElement.OpacityProperty, fade);
            auroraLast = -1.0;
        }

        private void ApplyAuroraSettingsTheme()
        {
            SolidColorBrush bright = new SolidColorBrush(Color.FromRgb(245, 250, 255));
            bright.Freeze();
            SolidColorBrush soft = new SolidColorBrush(Color.FromRgb(214, 226, 240));
            soft.Freeze();
            SolidColorBrush error = new SolidColorBrush(Color.FromRgb(252, 165, 165));
            error.Freeze();

            if (settingsTitle != null)
            {
                settingsTitle.Foreground = bright;
            }
            if (closeSettingsButton != null) closeSettingsButton.Content = CreateCloseIcon(16, bright);
            if (startFieldLabel != null)
            {
                startFieldLabel.Foreground = soft;
            }
            if (endFieldLabel != null)
            {
                endFieldLabel.Foreground = soft;
            }
            if (soundLabel != null)
            {
                soundLabel.Foreground = soft;
            }
            if (autoStartLabel != null)
            {
                autoStartLabel.Foreground = soft;
            }
            if (languageLabel != null)
            {
                languageLabel.Foreground = soft;
            }
            if (backgroundLabel != null)
            {
                backgroundLabel.Foreground = soft;
            }
            if (settingsError != null)
            {
                settingsError.Foreground = error;
            }
        }

        private void ApplyAuroraCollapsedTheme()
        {
            SolidColorBrush bright = new SolidColorBrush(Color.FromRgb(245, 250, 255));
            bright.Freeze();
            SolidColorBrush soft = new SolidColorBrush(Color.FromRgb(214, 226, 240));
            soft.Freeze();
            if (collapsedIcon != null) collapsedIcon.Content = CreateClockIcon(16, bright);
            if (collapsedLabel != null)
            {
                collapsedLabel.Foreground = soft;
            }
            if (collapsedTime != null)
            {
                collapsedTime.Foreground = bright;
            }
        }

        private void OnTick(object sender, EventArgs e)
        {
            RefreshUi();
        }

        private void OnDisplaySettingsChanged(object sender, EventArgs e)
        {
            Dispatcher.BeginInvoke(
                DispatcherPriority.Background,
                new Action(delegate
                {
                    if (!anchorReady) return;
                    if (!hasCustomPosition)
                    {
                        Rect area = SystemParameters.WorkArea;
                        anchorRight = area.Right;
                        anchorTop = area.Top;
                    }
                    if (dockEdge != DockEdge.None)
                    {
                        StopDockSlide();
                        RepositionDockedView();
                    }
                    else
                    {
                        ClampAnchor();
                        ApplyAnchor();
                        if (hasCustomPosition) PersistPosition();
                    }
                }));
        }

        private void OnClosing(object sender, CancelEventArgs e)
        {
            if (allowClose) return;
            e.Cancel = true;
            CancelDockHide();
            StopDockSlide();
            StopOffWorkReminder();
            Hide();
            UpdateTrayToggleText();
        }

        private void OnPreviewKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Escape && settingsOpen)
            {
                ShowCountdown();
                e.Handled = true;
            }
        }

        #region 界面

        private bool IsEnglish
        {
            get { return language == "en"; }
        }

        private string L(string chinese, string english)
        {
            return IsEnglish ? english : chinese;
        }

        private Grid BuildRoot()
        {
            Grid root = new Grid();
            root.Margin = new Thickness(WindowMargin);

            surface = new Grid();
            surface.HorizontalAlignment = HorizontalAlignment.Right;
            surface.VerticalAlignment = VerticalAlignment.Top;

            shadow = new Border();
            shadow.Background = Surface;
            if (AuroraEffect.IsAvailable)
            {
                SolidColorBrush darkSurface = new SolidColorBrush(Color.FromRgb(6, 12, 20));
                darkSurface.Freeze();
                shadow.Background = darkSurface;
            }
            shadow.CornerRadius = new CornerRadius(15);
            shadow.IsHitTestVisible = false;
            DropShadowEffect farShadow = new DropShadowEffect();
            farShadow.Color = Color.FromRgb(15, 23, 42);
            farShadow.BlurRadius = 22;
            farShadow.ShadowDepth = 3;
            farShadow.Direction = 270;
            farShadow.Opacity = 0.14;
            farShadow.RenderingBias = RenderingBias.Performance;
            shadow.Effect = farShadow;

            nearShadow = new Border();
            nearShadow.Background = Surface;
            if (backdropMode != "none") nearShadow.Background = new SolidColorBrush(Color.FromRgb(6, 12, 20));
            nearShadow.CornerRadius = new CornerRadius(15);
            nearShadow.IsHitTestVisible = false;
            DropShadowEffect contactShadow = new DropShadowEffect();
            contactShadow.Color = Color.FromRgb(15, 23, 42);
            contactShadow.BlurRadius = 8;
            contactShadow.ShadowDepth = 2;
            contactShadow.Direction = 270;
            contactShadow.Opacity = 0.09;
            contactShadow.RenderingBias = RenderingBias.Performance;
            nearShadow.Effect = contactShadow;

            card = new Border();
            if (AuroraEffect.IsAvailable)
            {
                card.Background = new SolidColorBrush(Color.FromRgb(6, 12, 20));
                card.BorderThickness = new Thickness(0);
            }
            else
            {
                card.Background = CardFill;
                card.BorderBrush = CardBorder;
                card.BorderThickness = new Thickness(1);
            }
            card.CornerRadius = new CornerRadius(15);
            card.SnapsToDevicePixels = true;
            card.Cursor = Cursors.SizeAll;

            countdownView = BuildCountdownView();
            settingsView = BuildSettingsView();
            collapsedView = BuildCollapsedView();
            // 三个视图都固定内容宽度并贴右上角：卡片做尺寸过渡时内容只被裁剪，不会重新排版。
            // Width 不含 Margin，所以两个带 18px 内边距的视图要减去各自的左右边距。
            countdownView.Width = ExpandedWidth - 2 - 36;
            settingsView.Width = ExpandedWidth - 2 - 38;
            collapsedView.Width = CollapsedWidth - 2;
            countdownView.HorizontalAlignment = HorizontalAlignment.Right;
            settingsView.HorizontalAlignment = HorizontalAlignment.Right;
            collapsedView.HorizontalAlignment = HorizontalAlignment.Right;
            countdownView.VerticalAlignment = VerticalAlignment.Top;
            settingsView.VerticalAlignment = VerticalAlignment.Top;
            collapsedView.VerticalAlignment = VerticalAlignment.Top;
            settingsView.Visibility = Visibility.Collapsed;
            collapsedView.Visibility = Visibility.Collapsed;
            settingsView.Opacity = 0;
            collapsedView.Opacity = 0;
            settingsView.IsHitTestVisible = false;
            collapsedView.IsHitTestVisible = false;
            currentView = countdownView;

            contentHost = new Grid();
            contentHost.ClipToBounds = true;
            contentHost.SizeChanged += delegate
            {
                double width = contentHost.ActualWidth;
                double height = contentHost.ActualHeight;
                if (width <= 0.0 || height <= 0.0) return;
                contentHost.Clip = new RectangleGeometry(new Rect(0.0, 0.0, width, height), 15.0, 15.0);
                if (auroraEffect != null) auroraEffect.Resolution = new Point(width, height);
                if (oceanEffect != null) oceanEffect.Resolution = new Point(width, height);
            };

            auroraEffect = new AuroraEffect();
            if (AuroraEffect.IsAvailable)
            {
                auroraRect = new Rectangle();
                auroraRect.Fill = Brushes.Black;
                auroraRect.Effect = auroraEffect;
                auroraRect.IsHitTestVisible = false;
                contentHost.Children.Add(auroraRect);
            }

            oceanEffect = new OceanEffect();
            if (OceanEffect.IsAvailable)
            {
                oceanRect = new Rectangle();
                oceanRect.Fill = Brushes.Black;
                oceanRect.Effect = oceanEffect;
                oceanRect.IsHitTestVisible = false;
                oceanRect.Visibility = Visibility.Collapsed;
                contentHost.Children.Add(oceanRect);
            }

            if (auroraRect != null) auroraRect.Visibility = backdropMode == "aurora" ? Visibility.Visible : Visibility.Collapsed;
            if (oceanRect != null) oceanRect.Visibility = backdropMode == "ocean" ? Visibility.Visible : Visibility.Collapsed;
            auroraActive = backdropMode != "none";

            sheenRect = new Rectangle();
            sheenRect.Height = 46;
            sheenRect.VerticalAlignment = VerticalAlignment.Top;
            sheenRect.Fill = SheenFill;
            sheenRect.IsHitTestVisible = false;
            sheenRect.Visibility = backdropMode == "none" ? Visibility.Visible : Visibility.Collapsed;
            contentHost.Children.Add(sheenRect);

            specularRect = new Rectangle();
            specularRect.Height = 1;
            specularRect.VerticalAlignment = VerticalAlignment.Top;
            specularRect.Margin = new Thickness(16, 0, 16, 0);
            specularRect.Fill = SpecularFill;
            specularRect.IsHitTestVisible = false;
            specularRect.Visibility = backdropMode == "none" ? Visibility.Visible : Visibility.Collapsed;
            contentHost.Children.Add(specularRect);

            contentHost.Children.Add(countdownView);
            contentHost.Children.Add(settingsView);
            contentHost.Children.Add(collapsedView);

            ApplyBackdropTheme();
            card.Child = contentHost;
            card.ContextMenu = BuildContextMenu();
            card.ContextMenu.Closed += delegate { ScheduleDockHide(); };
            card.MouseEnter += OnCardMouseEnter;
            card.MouseLeave += OnCardMouseLeave;

            surface.Children.Add(shadow);
            surface.Children.Add(nearShadow);
            surface.Children.Add(card);
            root.Children.Add(surface);

            surface.PreviewMouseLeftButtonDown += OnDragStart;
            surface.PreviewMouseMove += OnDragMove;
            surface.PreviewMouseLeftButtonUp += OnDragEnd;
            return root;
        }

        private StackPanel BuildCountdownView()
        {
            StackPanel view = new StackPanel();
            Brush headerBrush = Brushes.White;
            headerBrush.Freeze();
            view.Margin = new Thickness(18, 14, 18, 16);

            Grid header = new Grid();

            StackPanel status = new StackPanel();
            status.Orientation = Orientation.Horizontal;
            status.VerticalAlignment = VerticalAlignment.Center;

            headerIcon = new ContentControl();
            headerIcon.Width = 18;
            headerIcon.Height = 18;
            headerIcon.Margin = new Thickness(0, 0, 6, 0);
            headerIcon.VerticalAlignment = VerticalAlignment.Center;
            headerIcon.Content = CreateClockIcon(18, headerBrush);

            statusLabel = MakeText(L("距离下班", "Until off work"), 13, FontWeights.Normal, TextSecondary);
            status.Children.Add(headerIcon);
            status.Children.Add(statusLabel);

            StackPanel actions = new StackPanel();
            actions.Orientation = Orientation.Horizontal;
            actions.HorizontalAlignment = HorizontalAlignment.Right;
            actions.VerticalAlignment = VerticalAlignment.Top;

            settingsButton = CreateIconButton(
                CreateSettingsIcon(headerBrush),
                L("设置", "Settings"),
                delegate { ShowSettings(); });
            collapseButton = CreateIconButton(
                CreateMinusIcon(16, headerBrush),
                L("收起", "Collapse"),
                delegate { Collapse(); });
            if (AuroraEffect.IsAvailable)
            {
                if (settingsButton.Content is FrameworkElement) ((FrameworkElement)settingsButton.Content).Effect = CreateHeaderShadow();
                if (collapseButton.Content is FrameworkElement) ((FrameworkElement)collapseButton.Content).Effect = CreateHeaderShadow();
            }
            settingsButton.Margin = new Thickness(0, 0, 2, 0);
            actions.Children.Add(settingsButton);
            actions.Children.Add(collapseButton);

            header.Children.Add(status);
            header.Children.Add(actions);
            view.Children.Add(header);

            Grid timeHost = new Grid();
            timeHost.Height = 42;
            timeHost.Margin = new Thickness(0, 6, 0, 0);

            countdownText = MakeText("00:00:00", 34, FontWeights.SemiBold, TextPrimary);
            countdownText.FontFamily = PickEnglishFont();
            countdownText.VerticalAlignment = VerticalAlignment.Center;
            countdownText.LineHeight = 40;
            Typography.SetNumeralAlignment(countdownText, FontNumeralAlignment.Tabular);

            doneText = MakeText(L("今天辛苦了。", "Good work today."), 26, FontWeights.SemiBold, Success);
            doneText.VerticalAlignment = VerticalAlignment.Center;
            countdownText.RenderTransform = new TranslateTransform();
            doneText.RenderTransform = new TranslateTransform();
            doneText.Visibility = Visibility.Collapsed;

            timeHost.Children.Add(countdownText);
            timeHost.Children.Add(doneText);
            view.Children.Add(timeHost);

            progressTrack = new Grid();
            progressTrack.Height = 4;
            progressTrack.Margin = new Thickness(0, 12, 0, 0);
            progressTrack.ClipToBounds = true;
            progressTrack.SizeChanged += delegate { UpdateProgressWidth(); };

            Border trackBackground = new Border();
            trackBackground.Background = Track;
            trackBackground.CornerRadius = new CornerRadius(2);

            progressFill = new Border();
            progressFill.Background = ProgressFill;
            progressFill.CornerRadius = new CornerRadius(2);
            progressFill.HorizontalAlignment = HorizontalAlignment.Left;

            progressTrack.Children.Add(trackBackground);
            progressTrack.Children.Add(progressFill);
            view.Children.Add(progressTrack);

            Grid footer = new Grid();
            footer.Margin = new Thickness(0, 8, 0, 0);

            footerLeftText = new TextBlock();
            footerLeftText.FontSize = 13;
            footerLeftText.Foreground = TextSecondary;
            footerLeftText.VerticalAlignment = VerticalAlignment.Center;
            footerCaptionRun = new Run();
            footerTimeRun = new Run();
            footerTimeRun.FontFamily = PickEnglishFont();
            footerLeftText.Inlines.Add(footerCaptionRun);
            footerLeftText.Inlines.Add(footerTimeRun);

            footerRightText = MakeText("100%", 13, FontWeights.Normal, TextSecondary);
            footerRightText.FontFamily = PickEnglishFont();
            footerRightText.HorizontalAlignment = HorizontalAlignment.Right;
            footerRightText.Margin = new Thickness(0, 0, 2, 0);
            footer.Children.Add(footerLeftText);
            footer.Children.Add(footerRightText);
            view.Children.Add(footer);

            return view;
        }

        private StackPanel BuildSettingsView()
        {
            StackPanel view = new StackPanel();
            // 右侧多留 2px：125%/150% 缩放下输入框圆角边框刚好压在裁切线上
            view.Margin = new Thickness(18, 14, 20, 15);

            Grid header = new Grid();
            settingsTitle = MakeText(L("倒计时设置", "Countdown settings"), 15.5, FontWeights.SemiBold, TextPrimary);
            settingsTitle.VerticalAlignment = VerticalAlignment.Center;
            closeSettingsButton = CreateIconButton(
                CreateCloseIcon(16, TextSecondary),
                L("关闭设置", "Close settings"),
                delegate { ShowCountdown(); });
            closeSettingsButton.HorizontalAlignment = HorizontalAlignment.Right;
            header.Children.Add(settingsTitle);
            header.Children.Add(closeSettingsButton);
            view.Children.Add(header);

            settingsError = MakeText(string.Empty, 11, FontWeights.Normal, TextDisabled);
            settingsError.Margin = new Thickness(0, 6, 0, 0);
            settingsError.MinHeight = 0;
            settingsError.TextWrapping = TextWrapping.Wrap;
            settingsError.Visibility = Visibility.Collapsed;

            Grid fields = new Grid();
            fields.Margin = new Thickness(0, 12, 0, 0);
            fields.ColumnDefinitions.Add(new ColumnDefinition());
            fields.ColumnDefinitions.Add(new ColumnDefinition());

            Border startBorder = CreateTimeInput(out startBox);
            Border endBorder = CreateTimeInput(out endBox);
            startBox.Text = settings.StartTime;
            endBox.Text = settings.EndTime;

            StackPanel startField = new StackPanel();
            startFieldLabel = MakeText(L("上班时间", "Start time"), 12.5, FontWeights.Normal, TextSecondary);
            startField.Children.Add(startFieldLabel);
            startBorder.Margin = new Thickness(0, 5, 0, 0);
            startField.Children.Add(startBorder);

            StackPanel endField = new StackPanel();
            endField.Margin = new Thickness(10, 0, 4, 0);
            endFieldLabel = MakeText(L("下班时间", "End time"), 12.5, FontWeights.Normal, TextSecondary);
            StackPanel endLabelRow = new StackPanel();
            endLabelRow.Orientation = Orientation.Horizontal;
            endLabelRow.Children.Add(endFieldLabel);
            endFieldBadge = CreateOvernightBadge(out endFieldBadgeText);
            endFieldBadge.Margin = new Thickness(6, 0, 0, 0);
            endLabelRow.Children.Add(endFieldBadge);
            endField.Children.Add(endLabelRow);
            endBorder.Margin = new Thickness(0, 5, 0, 0);
            endField.Children.Add(endBorder);

            Grid.SetColumn(startField, 0);
            Grid.SetColumn(endField, 1);
            fields.Children.Add(startField);
            fields.Children.Add(endField);
            view.Children.Add(fields);

            Grid soundRow = new Grid();
            soundRow.Margin = new Thickness(0, 10, 4, 0);

            soundLabel = MakeText(L("下班提醒音", "End-of-day sound"), 12.5, FontWeights.Normal, TextSecondary);
            soundLabel.VerticalAlignment = VerticalAlignment.Center;

            soundToggle = CreateToggleSwitch();
            soundToggle.HorizontalAlignment = HorizontalAlignment.Right;
            soundToggle.VerticalAlignment = VerticalAlignment.Center;
            soundToggle.IsChecked = settings.SoundEnabled;
            AutomationProperties.SetName(
                soundToggle,
                L("开启下班提醒音", "Enable end-of-day sound"));
            soundToggle.Checked += delegate
            {
                if (suppressSettingsApply || settings.SoundEnabled) return;
                settings.SoundEnabled = true;
                SettingsStore.Save(settings);
            };
            soundToggle.Unchecked += delegate
            {
                if (suppressSettingsApply || !settings.SoundEnabled) return;
                settings.SoundEnabled = false;
                SettingsStore.Save(settings);
            };

            soundRow.Children.Add(soundLabel);
            soundRow.Children.Add(soundToggle);
            view.Children.Add(soundRow);

            Grid autoRow = new Grid();
            autoRow.Margin = new Thickness(0, 6, 4, 0);

            autoStartLabel = MakeText(L("开机自启", "Start with Windows"), 12.5, FontWeights.Normal, TextSecondary);
            autoStartLabel.VerticalAlignment = VerticalAlignment.Center;

            autoStartToggle = CreateToggleSwitch();
            autoStartToggle.HorizontalAlignment = HorizontalAlignment.Right;
            autoStartToggle.VerticalAlignment = VerticalAlignment.Center;
            autoStartToggle.IsChecked = IsAutoStartEnabled();
            AutomationProperties.SetName(
                autoStartToggle,
                L("开机自启开关", "Start with Windows toggle"));
            autoStartToggle.Checked += delegate
            {
                if (suppressSettingsApply) return;
                SetAutoStart(true);
            };
            autoStartToggle.Unchecked += delegate
            {
                if (suppressSettingsApply) return;
                SetAutoStart(false);
            };

            autoRow.Children.Add(autoStartLabel);
            autoRow.Children.Add(autoStartToggle);
            view.Children.Add(autoRow);

            Grid languageRow = new Grid();
            languageRow.Margin = new Thickness(0, 8, 4, 0);

            languageLabel = MakeText(L("语言", "Language"), 12.5, FontWeights.Normal, TextSecondary);
            languageLabel.VerticalAlignment = VerticalAlignment.Center;

            Border languageTrack = new Border();
            languageTrack.Height = 32;
            languageTrack.Padding = new Thickness(2);
            languageTrack.Background = SurfaceMuted;
            languageTrack.BorderBrush = BorderLine;
            languageTrack.BorderThickness = new Thickness(1);
            languageTrack.CornerRadius = new CornerRadius(9);
            languageTrack.HorizontalAlignment = HorizontalAlignment.Right;
            languageTrack.VerticalAlignment = VerticalAlignment.Center;

            Grid languageGrid = new Grid();
            languageGrid.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(56)
            });
            languageGrid.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(56)
            });

            languageZhButton = CreateLanguageSegmentButton(
                "中文",
                delegate { SetLanguage("zh"); });
            languageEnButton = CreateLanguageSegmentButton(
                "English",
                delegate { SetLanguage("en"); });
            Grid.SetColumn(languageEnButton, 1);
            languageThumb = new Border();
            languageThumb.Width = 56;
            languageThumb.Height = 24;
            languageThumb.CornerRadius = new CornerRadius(7);
            languageThumb.Background = Brand;
            languageThumb.HorizontalAlignment = HorizontalAlignment.Left;
            languageThumb.VerticalAlignment = VerticalAlignment.Center;
            languageThumb.IsHitTestVisible = false;
            languageThumb.RenderTransform = new TranslateTransform();
            Grid.SetColumn(languageThumb, 0);
            languageGrid.Children.Add(languageThumb);
            languageGrid.Children.Add(languageZhButton);
            languageGrid.Children.Add(languageEnButton);
            languageTrack.Child = languageGrid;

            languageRow.Children.Add(languageLabel);
            languageRow.Children.Add(languageTrack);
            view.Children.Add(languageRow);

            Grid backdropRow = new Grid();
            backdropRow.Margin = new Thickness(0, 8, 4, 0);

            backgroundLabel = MakeText(L("背景", "Background"), 12.5, FontWeights.Normal, TextSecondary);
            backgroundLabel.VerticalAlignment = VerticalAlignment.Center;

            Border backdropTrack = new Border();
            backdropTrack.Height = 32;
            backdropTrack.Padding = new Thickness(2);
            backdropTrack.Background = SurfaceMuted;
            backdropTrack.BorderBrush = BorderLine;
            backdropTrack.BorderThickness = new Thickness(1);
            backdropTrack.CornerRadius = new CornerRadius(9);
            backdropTrack.HorizontalAlignment = HorizontalAlignment.Right;
            backdropTrack.VerticalAlignment = VerticalAlignment.Center;

            Grid backdropGrid = new Grid();
            backdropGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(56) });
            backdropGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(56) });
            backdropGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(56) });

            backgroundNoneButton = CreateLanguageSegmentButton(
                L("无背景", "None"),
                delegate { SetBackdropMode("none"); });
            backgroundAuroraButton = CreateLanguageSegmentButton(
                L("极光", "Aurora"),
                delegate { SetBackdropMode("aurora"); });
            backgroundOceanButton = CreateLanguageSegmentButton(
                L("大海", "Ocean"),
                delegate { SetBackdropMode("ocean"); });
            Grid.SetColumn(backgroundAuroraButton, 1);
            Grid.SetColumn(backgroundOceanButton, 2);

            backgroundThumb = new Border();
            backgroundThumb.Width = 56;
            backgroundThumb.Height = 24;
            backgroundThumb.CornerRadius = new CornerRadius(7);
            backgroundThumb.Background = Brand;
            backgroundThumb.HorizontalAlignment = HorizontalAlignment.Left;
            backgroundThumb.VerticalAlignment = VerticalAlignment.Center;
            backgroundThumb.IsHitTestVisible = false;
            backgroundThumb.RenderTransform = new TranslateTransform();
            Grid.SetColumn(backgroundThumb, 0);

            backdropGrid.Children.Add(backgroundThumb);
            backdropGrid.Children.Add(backgroundNoneButton);
            backdropGrid.Children.Add(backgroundAuroraButton);
            backdropGrid.Children.Add(backgroundOceanButton);
            backdropTrack.Child = backdropGrid;

            backdropRow.Children.Add(backgroundLabel);
            backdropRow.Children.Add(backdropTrack);
            view.Children.Add(backdropRow);

            view.Children.Add(settingsError);
            UpdateBackdropSegments();
            UpdateLanguageSegments();

            startBox.KeyDown += OnSettingsKeyDown;
            endBox.KeyDown += OnSettingsKeyDown;
            AutomationProperties.SetName(startBox, L("上班时间", "Start time"));
            AutomationProperties.SetName(endBox, L("下班时间", "End time"));

            return view;
        }

        private void SetBackdropMode(string mode)
        {
            string normalized = NormalizeBackdrop(mode);
            if (normalized == backdropMode)
            {
                UpdateBackdropSegments();
                return;
            }
            backdropMode = normalized;
            settings.Background = normalized;
            SettingsStore.Save(settings);
            if (auroraRect != null) auroraRect.Visibility = normalized == "aurora" ? Visibility.Visible : Visibility.Collapsed;
            if (oceanRect != null) oceanRect.Visibility = normalized == "ocean" ? Visibility.Visible : Visibility.Collapsed;
            auroraActive = normalized != "none";
            ApplyBackdropTheme();
            UpdateBackdropSegments();
        }

        private void UpdateBackdropSegments()
        {
            if (backgroundNoneButton == null || backgroundThumb == null) return;
            int index = backdropMode == "none" ? 0 : (backdropMode == "ocean" ? 2 : 1);
            TranslateTransform shift = backgroundThumb.RenderTransform as TranslateTransform;
            if (shift == null)
            {
                shift = new TranslateTransform();
                backgroundThumb.RenderTransform = shift;
            }
            double target = index * 56.0;
            if (IsVisible && currentView == settingsView && SystemParameters.ClientAreaAnimation)
            {
                DoubleAnimation slide = new DoubleAnimation(target, TimeSpan.FromMilliseconds(180));
                slide.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
                shift.BeginAnimation(TranslateTransform.XProperty, slide);
            }
            else
            {
                shift.BeginAnimation(TranslateTransform.XProperty, null);
                shift.X = target;
            }
            backgroundNoneButton.FontFamily = IsEnglish ? PickEnglishFont() : PickChineseFont();
            backgroundAuroraButton.FontFamily = IsEnglish ? PickEnglishFont() : PickChineseFont();
            backgroundOceanButton.FontFamily = IsEnglish ? PickEnglishFont() : PickChineseFont();
            SetBackdropButtonState(backgroundNoneButton, index == 0);
            SetBackdropButtonState(backgroundAuroraButton, index == 1);
            SetBackdropButtonState(backgroundOceanButton, index == 2);
        }

        private static void SetBackdropButtonState(Button button, bool selected)
        {
            if (button == null) return;
            button.Foreground = selected ? Brushes.White : TextSecondary;
            button.FontWeight = selected ? FontWeights.SemiBold : FontWeights.Normal;
        }

        private Button CreateLanguageSegmentButton(string text, RoutedEventHandler click)
        {
            Button button = new Button();
            button.Content = text;
            button.Height = 24;
            button.MinWidth = 56;
            button.Padding = new Thickness(4, 0, 4, 0);
            button.FontSize = 11.5;
            button.BorderThickness = new Thickness(0);
            button.Background = Brushes.Transparent;
            button.Cursor = Cursors.Hand;
            button.Focusable = false;
            button.Click += click;
            AutomationProperties.SetName(button, text);

            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory background = new FrameworkElementFactory(typeof(Border));
            background.SetValue(Border.CornerRadiusProperty, new CornerRadius(6));
            background.SetValue(
                Border.BackgroundProperty,
                new TemplateBindingExtension(Control.BackgroundProperty));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            background.AppendChild(presenter);
            template.VisualTree = background;
            button.Template = template;
            return button;
        }

        private void SetLanguage(string newLanguage)
        {
            if (newLanguage != "en") newLanguage = "zh";
            if (language == newLanguage)
            {
                UpdateLanguageSegments();
                return;
            }

            language = newLanguage;
            settings.Language = newLanguage;
            SettingsStore.Save(settings);
            ResetSettingsMessage();

            if (IsVisible && currentView == settingsView && SystemParameters.ClientAreaAnimation)
            {
                PlayLanguageTransition();
            }
            else
            {
                ApplyLanguage();
            }
        }

        private void PlayLanguageTransition()
        {
            if (contentHost == null)
            {
                ApplyLanguage();
                return;
            }

            BlurEffect blur = contentHost.Effect as BlurEffect;
            if (blur == null)
            {
                blur = new BlurEffect();
                blur.KernelType = KernelType.Gaussian;
                blur.RenderingBias = RenderingBias.Performance;
                blur.Radius = 0;
                contentHost.Effect = blur;
            }
            ScaleTransform scale = contentHost.RenderTransform as ScaleTransform;
            if (scale == null)
            {
                scale = new ScaleTransform(1, 1);
                contentHost.RenderTransformOrigin = new Point(0.5, 0.5);
                contentHost.RenderTransform = scale;
            }

            DoubleAnimation blurOut = new DoubleAnimation(
                0,
                4.5,
                TimeSpan.FromMilliseconds(110));
            blurOut.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseIn };
            blurOut.Completed += delegate
            {
                // 模糊到峰值时替换文字，再模糊回来，切换过程不会生硬跳变。
                ApplyLanguage();
                DoubleAnimation blurIn = new DoubleAnimation(
                    4.5,
                    0,
                    TimeSpan.FromMilliseconds(220));
                blurIn.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
                blur.BeginAnimation(BlurEffect.RadiusProperty, blurIn);

                DoubleAnimation fadeIn = new DoubleAnimation(
                    0.55,
                    1,
                    TimeSpan.FromMilliseconds(220));
                fadeIn.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
                contentHost.BeginAnimation(UIElement.OpacityProperty, fadeIn);

                DoubleAnimation scaleIn = new DoubleAnimation(
                    0.99,
                    1,
                    TimeSpan.FromMilliseconds(220));
                scaleIn.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
                scale.BeginAnimation(ScaleTransform.ScaleXProperty, scaleIn);
                scale.BeginAnimation(ScaleTransform.ScaleYProperty, scaleIn);
            };
            blur.BeginAnimation(BlurEffect.RadiusProperty, blurOut);

            DoubleAnimation fadeOut = new DoubleAnimation(
                1,
                0.55,
                TimeSpan.FromMilliseconds(110));
            fadeOut.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseIn };
            contentHost.BeginAnimation(UIElement.OpacityProperty, fadeOut);

            DoubleAnimation scaleOut = new DoubleAnimation(
                1,
                0.99,
                TimeSpan.FromMilliseconds(110));
            scaleOut.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseIn };
            scale.BeginAnimation(ScaleTransform.ScaleXProperty, scaleOut);
            scale.BeginAnimation(ScaleTransform.ScaleYProperty, scaleOut);
        }

        private void UpdateLanguageSegments()
        {
            if (languageZhButton == null || languageEnButton == null) return;
            bool english = IsEnglish;
            languageZhButton.FontFamily = PickChineseFont();
            languageEnButton.FontFamily = PickEnglishFont();

            if (languageThumb != null)
            {
                TranslateTransform shift = languageThumb.RenderTransform as TranslateTransform;
                if (shift == null)
                {
                    shift = new TranslateTransform();
                    languageThumb.RenderTransform = shift;
                }
                double target = english ? 56 : 0;
                if (IsVisible && currentView == settingsView)
                {
                    DoubleAnimation slide = new DoubleAnimation(
                        target,
                        TimeSpan.FromMilliseconds(180));
                    slide.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
                    shift.BeginAnimation(TranslateTransform.XProperty, slide);
                }
                else
                {
                    shift.BeginAnimation(TranslateTransform.XProperty, null);
                    shift.X = target;
                }
            }

            languageZhButton.Background = Brushes.Transparent;
            languageEnButton.Background = Brushes.Transparent;
            languageZhButton.Foreground = english ? TextSecondary : Brushes.White;
            languageZhButton.FontWeight = english ? FontWeights.Normal : FontWeights.SemiBold;
            languageEnButton.Foreground = english ? Brushes.White : TextSecondary;
            languageEnButton.FontWeight = english ? FontWeights.SemiBold : FontWeights.Normal;
        }

        private static CheckBox CreateToggleSwitch()
        {
            CheckBox box = new CheckBox();
            box.Width = 34;
            box.Height = 18;
            box.Focusable = false;
            box.Cursor = Cursors.Hand;
            box.FocusVisualStyle = null;

            ControlTemplate template = new ControlTemplate(typeof(CheckBox));
            FrameworkElementFactory track = new FrameworkElementFactory(typeof(Border));
            track.Name = "track";
            track.SetValue(Border.CornerRadiusProperty, new CornerRadius(9));
            track.SetValue(Border.BackgroundProperty, Track);

            FrameworkElementFactory knob = new FrameworkElementFactory(typeof(Border));
            knob.Name = "knob";
            knob.SetValue(FrameworkElement.WidthProperty, 14.0);
            knob.SetValue(FrameworkElement.HeightProperty, 14.0);
            knob.SetValue(Border.CornerRadiusProperty, new CornerRadius(7));
            knob.SetValue(Border.BackgroundProperty, Brushes.White);
            knob.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Left);
            knob.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            knob.SetValue(FrameworkElement.MarginProperty, new Thickness(2, 0, 0, 0));
            knob.SetValue(UIElement.RenderTransformProperty, new TranslateTransform());
            track.AppendChild(knob);
            template.VisualTree = track;

            Trigger on = new Trigger();
            on.Property = ToggleButton.IsCheckedProperty;
            on.Value = true;
            on.Setters.Add(new Setter(Border.BackgroundProperty, Brand, "track"));

            Storyboard enterStoryboard = new Storyboard();
            DoubleAnimation enterKnob = new DoubleAnimation(
                0,
                16,
                TimeSpan.FromMilliseconds(170));
            enterKnob.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            Storyboard.SetTargetName(enterKnob, "knob");
            Storyboard.SetTargetProperty(
                enterKnob,
                new PropertyPath("(UIElement.RenderTransform).(TranslateTransform.X)"));
            enterStoryboard.Children.Add(enterKnob);
            on.EnterActions.Add(new BeginStoryboard { Storyboard = enterStoryboard });

            Storyboard exitStoryboard = new Storyboard();
            DoubleAnimation exitKnob = new DoubleAnimation(
                16,
                0,
                TimeSpan.FromMilliseconds(170));
            exitKnob.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            Storyboard.SetTargetName(exitKnob, "knob");
            Storyboard.SetTargetProperty(
                exitKnob,
                new PropertyPath("(UIElement.RenderTransform).(TranslateTransform.X)"));
            exitStoryboard.Children.Add(exitKnob);
            on.ExitActions.Add(new BeginStoryboard { Storyboard = exitStoryboard });

            template.Triggers.Add(on);

            box.Template = template;
            return box;
        }

        private Button BuildCollapsedView()
        {
            StackPanel content = new StackPanel();
            content.Orientation = Orientation.Horizontal;
            content.Margin = new Thickness(14, 9, 14, 9);
            content.Background = Brushes.Transparent;

            collapsedIcon = new ContentControl();
            collapsedIcon.Width = 16;
            collapsedIcon.Height = 16;
            collapsedIcon.Margin = new Thickness(0, 0, 8, 0);
            collapsedIcon.VerticalAlignment = VerticalAlignment.Center;
            collapsedIcon.Content = CreateClockIcon(16, TextSecondary);

            collapsedLabel = MakeText(L("距离下班", "Until off work"), 12.5, FontWeights.Normal, TextSecondary);
            collapsedLabel.VerticalAlignment = VerticalAlignment.Center;
            collapsedLabel.Margin = new Thickness(0, 0, 6, 0);

            collapsedTime = MakeText("00:00:00", 15, FontWeights.SemiBold, TextPrimary);
            collapsedTime.FontFamily = PickEnglishFont();
            collapsedTime.VerticalAlignment = VerticalAlignment.Center;

            content.Children.Add(collapsedIcon);
            content.Children.Add(collapsedLabel);
            content.Children.Add(collapsedTime);

            Button view = new Button();
            view.Content = content;
            view.Cursor = Cursors.Hand;
            view.Background = Brushes.Transparent;
            view.BorderThickness = new Thickness(0);
            view.Padding = new Thickness(0);
            view.HorizontalContentAlignment = HorizontalAlignment.Stretch;
            view.VerticalContentAlignment = VerticalAlignment.Center;
            view.Template = CreateBlankButtonTemplate();
            view.Click += delegate { ShowCountdown(); };
            ToolTipService.SetToolTip(view, L("拖动调整位置，点击展开", "Drag to move, click to expand"));
            AutomationProperties.SetName(view, L("展开下班倒计时", "Expand countdown"));
            return view;
        }

        private static ControlTemplate CreateBlankButtonTemplate()
        {
            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory border = new FrameworkElementFactory(typeof(Border));
            border.SetValue(Border.BackgroundProperty, Brushes.Transparent);
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Stretch);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            border.AppendChild(presenter);
            template.VisualTree = border;
            return template;
        }

        private ContextMenu BuildContextMenu()
        {
            ContextMenu menu = new ContextMenu();

            MenuItem settingsItem = new MenuItem();
            contextSettingsItem = settingsItem;
            settingsItem.Header = L("设置", "Settings");
            settingsItem.Click += delegate { ShowSettings(); };

            MenuItem hideItem = new MenuItem();
            contextHideItem = hideItem;
            hideItem.Header = L("隐藏窗口", "Hide window");
            hideItem.Click += delegate
            {
                CancelDockHide();
                StopDockSlide();
                StopOffWorkReminder();
                Hide();
                UpdateTrayToggleText();
            };

            MenuItem exitItem = new MenuItem();
            contextExitItem = exitItem;
            exitItem.Header = L("退出", "Exit");
            exitItem.Click += delegate { ExitApp(); };

            menu.Items.Add(settingsItem);
            menu.Items.Add(hideItem);
            menu.Items.Add(new Separator());
            menu.Items.Add(exitItem);
            return menu;
        }

        private static TextBlock MakeText(string text, double size, FontWeight weight, Brush foreground)
        {
            TextBlock block = new TextBlock();
            block.Text = text;
            block.FontSize = size;
            block.FontWeight = weight;
            block.Foreground = foreground;
            block.TextWrapping = TextWrapping.NoWrap;
            return block;
        }

        private static Button CreateIconButton(
            FrameworkElement icon,
            string tooltip,
            RoutedEventHandler click)
        {
            icon.Width = 16;
            icon.Height = 16;

            Button button = new Button();
            button.Content = icon;
            button.Width = 24;
            button.Height = 24;
            button.Padding = new Thickness(0);
            button.Margin = new Thickness(0);
            button.Background = Brushes.Transparent;
            button.BorderThickness = new Thickness(0);
            button.Cursor = Cursors.Hand;
            button.Click += click;
            ToolTipService.SetToolTip(button, tooltip);
            AutomationProperties.SetName(button, tooltip);

            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory border = new FrameworkElementFactory(typeof(Border));
            border.Name = "bg";
            border.SetValue(Border.BackgroundProperty, Brushes.Transparent);
            border.SetValue(Border.CornerRadiusProperty, new CornerRadius(6));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            border.AppendChild(presenter);
            template.VisualTree = border;

            Brush iconHover = HoverFill;
            Brush iconPressed = PressedFill;
            if (AuroraEffect.IsAvailable)
            {
                iconHover = new SolidColorBrush(Color.FromArgb(70, 0, 0, 0));
                iconPressed = new SolidColorBrush(Color.FromArgb(110, 0, 0, 0));
            }

            Trigger hover = new Trigger();
            hover.Property = UIElement.IsMouseOverProperty;
            hover.Value = true;
            hover.Setters.Add(new Setter(Border.BackgroundProperty, iconHover, "bg"));
            template.Triggers.Add(hover);

            Trigger pressed = new Trigger();
            pressed.Property = ButtonBase.IsPressedProperty;
            pressed.Value = true;
            pressed.Setters.Add(new Setter(Border.BackgroundProperty, iconPressed, "bg"));
            template.Triggers.Add(pressed);

            button.Template = template;

            Style style = new Style(typeof(Button));
            style.Setters.Add(new Setter(UIElement.OpacityProperty, 0.7));
            Trigger hoverOpacity = new Trigger();
            hoverOpacity.Property = UIElement.IsMouseOverProperty;
            hoverOpacity.Value = true;
            hoverOpacity.Setters.Add(new Setter(UIElement.OpacityProperty, 1.0));
            style.Triggers.Add(hoverOpacity);
            button.Style = style;
            return button;
        }

        private static Button CreateActionButton(
            string text,
            bool primary,
            RoutedEventHandler click)
        {
            SolidColorBrush baseBrush = primary ? Brand : Brushes.Transparent;
            SolidColorBrush hoverBrush = primary ? BrandHover : HoverFill;
            SolidColorBrush pressedBrush = primary ? BrandPressed : PressedFill;

            Button button = new Button();
            button.Content = text;
            button.Height = 30;
            button.MinWidth = primary ? 56 : 44;
            button.Padding = new Thickness(12, 0, 12, 0);
            button.FontSize = 12;
            button.FontWeight = primary ? FontWeights.SemiBold : FontWeights.Normal;
            button.Foreground = primary ? Brushes.White : TextSecondary;
            button.Background = baseBrush;
            button.BorderThickness = new Thickness(0);
            button.Cursor = Cursors.Hand;
            button.Click += click;

            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory border = new FrameworkElementFactory(typeof(Border));
            border.Name = "bg";
            border.SetValue(Border.BackgroundProperty, baseBrush);
            border.SetValue(Border.CornerRadiusProperty, new CornerRadius(8));
            border.SetValue(Border.PaddingProperty, new Thickness(12, 0, 12, 0));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            border.AppendChild(presenter);
            template.VisualTree = border;

            Trigger hover = new Trigger();
            hover.Property = UIElement.IsMouseOverProperty;
            hover.Value = true;
            hover.Setters.Add(new Setter(Border.BackgroundProperty, hoverBrush, "bg"));
            template.Triggers.Add(hover);

            Trigger pressed = new Trigger();
            pressed.Property = ButtonBase.IsPressedProperty;
            pressed.Value = true;
            pressed.Setters.Add(new Setter(Border.BackgroundProperty, pressedBrush, "bg"));
            template.Triggers.Add(pressed);

            button.Template = template;
            return button;
        }

        private static Button CreateLinkButton(string text, RoutedEventHandler click)
        {
            Button button = new Button();
            button.Content = text;
            button.Height = 30;
            button.Padding = new Thickness(2, 0, 2, 0);
            button.FontSize = 12;
            button.Background = Brushes.Transparent;
            button.BorderThickness = new Thickness(0);
            button.Cursor = Cursors.Hand;
            button.Click += click;

            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            template.VisualTree = presenter;
            button.Template = template;

            Style style = new Style(typeof(Button));
            style.Setters.Add(new Setter(Control.ForegroundProperty, TextSecondary));
            Trigger hover = new Trigger();
            hover.Property = UIElement.IsMouseOverProperty;
            hover.Value = true;
            hover.Setters.Add(new Setter(Control.ForegroundProperty, Brand));
            style.Triggers.Add(hover);
            button.Style = style;
            return button;
        }

        private static Border CreateOvernightBadge(out TextBlock label)
        {
            Border badge = new Border();
            badge.Background = OvernightBadgeFill;
            badge.CornerRadius = new CornerRadius(8);
            badge.Height = 18;
            badge.Padding = new Thickness(7, 0, 7, 0);
            badge.VerticalAlignment = VerticalAlignment.Center;
            badge.Visibility = Visibility.Collapsed;
            label = MakeText("次日", 10.5, FontWeights.Normal, Brand);
            label.VerticalAlignment = VerticalAlignment.Center;
            badge.Child = label;
            return badge;
        }

        private void UpdateOvernightBadge()
        {
            if (endFieldBadge == null || startBox == null || endBox == null) return;
            if (endFieldBadgeText != null)
            {
                endFieldBadgeText.Text = L("次日", "Next day");
            }
            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            bool overnight =
                CountdownSettings.TryParseFlexibleTime(startBox.Text, out startText, out start) &&
                CountdownSettings.TryParseFlexibleTime(endBox.Text, out endText, out end) &&
                end <= start;
            endFieldBadge.Visibility = overnight ? Visibility.Visible : Visibility.Collapsed;
        }
        private Border CreateTimeInput(out TextBox input)
        {
            Border shell = new Border();
            shell.Height = 38;
            shell.Background = SurfaceMuted;
            shell.BorderBrush = BorderLine;
            shell.BorderThickness = new Thickness(1);
            shell.CornerRadius = new CornerRadius(9);

            Grid content = new Grid();
            content.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(26)
            });
            content.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(1)
            });
            content.ColumnDefinitions.Add(new ColumnDefinition());
            content.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(1)
            });
            content.ColumnDefinitions.Add(new ColumnDefinition
            {
                Width = new GridLength(26)
            });

            input = new TextBox();
            input.MaxLength = 5;
            input.FontSize = 14.5;
            input.FontFamily = PickEnglishFont();
            input.Foreground = TextPrimary;
            input.CaretBrush = TextPrimary;
            input.Background = Brushes.Transparent;
            input.BorderThickness = new Thickness(0);
            input.Padding = new Thickness(0);
            input.Margin = new Thickness(4, 0, 4, 0);
            input.VerticalContentAlignment = VerticalAlignment.Center;
            input.TextAlignment = TextAlignment.Center;
            input.FocusVisualStyle = null;
            Grid.SetColumn(input, 2);
            content.Children.Add(input);
            TextBox inputBox = input;

            Button minus = CreateTimeStepButton(false, input, -30, true);
            Grid.SetColumn(minus, 0);
            content.Children.Add(minus);

            Border leftDivider = new Border();
            leftDivider.Background = StepperLine;
            Grid.SetColumn(leftDivider, 1);
            content.Children.Add(leftDivider);

            Border rightDivider = new Border();
            rightDivider.Background = StepperLine;
            Grid.SetColumn(rightDivider, 3);
            content.Children.Add(rightDivider);

            Button plus = CreateTimeStepButton(true, input, 30, false);
            Grid.SetColumn(plus, 4);
            content.Children.Add(plus);

            shell.Child = content;

            input.GotFocus += delegate
            {
                shell.Background = Surface;
                shell.BorderBrush = Brand;
            };
            input.LostFocus += delegate
            {
                shell.Background = SurfaceMuted;
                shell.BorderBrush = BorderLine;
                NormalizeTimeBox(inputBox);
                ApplyTimeSettings();
            };
            input.TextChanged += delegate
            {
                ResetSettingsMessage();
                UpdateOvernightBadge();
            };
            return shell;
        }

        private Button CreateTimeStepButton(bool plus, TextBox input, int minutes, bool left)
        {
            Button button = new Button();
            button.Content = CreateStepperIcon(plus, 18, TextSecondary);
            button.Width = 26;
            button.Height = 36;
            button.Padding = new Thickness(0);
            button.Background = StepperFill;
            button.BorderThickness = new Thickness(0);
            button.Focusable = false;
            button.Cursor = Cursors.Hand;
            button.Click += delegate
            {
                BumpTime(input, minutes);
                ApplyStepperTime(input == endBox);
            };

            ControlTemplate template = new ControlTemplate(typeof(Button));
            FrameworkElementFactory background = new FrameworkElementFactory(typeof(Border));
            background.Name = "zone";
            background.SetValue(Border.BackgroundProperty, StepperFill);
            background.SetValue(
                Border.CornerRadiusProperty,
                left ? new CornerRadius(8, 0, 0, 8) : new CornerRadius(0, 8, 8, 0));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            background.AppendChild(presenter);
            template.VisualTree = background;

            Trigger hover = new Trigger();
            hover.Property = UIElement.IsMouseOverProperty;
            hover.Value = true;
            hover.Setters.Add(new Setter(Border.BackgroundProperty, HoverFill, "zone"));
            template.Triggers.Add(hover);

            Trigger pressed = new Trigger();
            pressed.Property = ButtonBase.IsPressedProperty;
            pressed.Value = true;
            pressed.Setters.Add(new Setter(Border.BackgroundProperty, StepperLine, "zone"));
            template.Triggers.Add(pressed);

            button.Template = template;
            return button;
        }

        private static Viewbox CreateStepperIcon(bool plus, double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 18;
            canvas.Height = 18;

            Line horizontal = new Line();
            horizontal.X1 = 3.5;
            horizontal.Y1 = 9;
            horizontal.X2 = 14.5;
            horizontal.Y2 = 9;
            horizontal.Stroke = stroke;
            horizontal.StrokeThickness = 2;
            horizontal.StrokeStartLineCap = PenLineCap.Round;
            horizontal.StrokeEndLineCap = PenLineCap.Round;
            canvas.Children.Add(horizontal);

            if (plus)
            {
                Line vertical = new Line();
                vertical.X1 = 9;
                vertical.Y1 = 3.5;
                vertical.X2 = 9;
                vertical.Y2 = 14.5;
                vertical.Stroke = stroke;
                vertical.StrokeThickness = 2;
                vertical.StrokeStartLineCap = PenLineCap.Round;
                vertical.StrokeEndLineCap = PenLineCap.Round;
                canvas.Children.Add(vertical);
            }

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static void BumpTime(TextBox input, int minutes)
        {
            string normalized;
            TimeSpan time;
            if (!CountdownSettings.TryParseFlexibleTime(input.Text, out normalized, out time))
            {
                time = TimeSpan.Zero;
            }

            int total = (int)time.TotalMinutes + minutes;
            total %= 24 * 60;
            if (total < 0) total += 24 * 60;
            input.Text = string.Format(
                CultureInfo.InvariantCulture,
                "{0:00}:{1:00}",
                total / 60,
                total % 60);
            input.CaretIndex = input.Text.Length;
        }

        private static void NormalizeTimeBox(TextBox input)
        {
            if (input == null) return;
            string normalized;
            TimeSpan time;
            if (CountdownSettings.TryParseFlexibleTime(input.Text, out normalized, out time) &&
                normalized != input.Text)
            {
                input.Text = normalized;
                input.CaretIndex = input.Text.Length;
            }
        }

        private static bool IsAutoStartEnabled()
        {
            try
            {
                using (Microsoft.Win32.RegistryKey key =
                    Microsoft.Win32.Registry.CurrentUser.OpenSubKey(AutoStartKeyPath, false))
                {
                    return key != null && key.GetValue(AutoStartValueName) != null;
                }
            }
            catch
            {
                return false;
            }
        }

        private static void SetAutoStart(bool enabled)
        {
            try
            {
                using (Microsoft.Win32.RegistryKey key =
                    Microsoft.Win32.Registry.CurrentUser.CreateSubKey(AutoStartKeyPath))
                {
                    if (key == null) return;
                    if (enabled)
                    {
                        key.SetValue(
                            AutoStartValueName,
                            "\"" + WinForms.Application.ExecutablePath + "\"");
                    }
                    else if (key.GetValue(AutoStartValueName) != null)
                    {
                        key.DeleteValue(AutoStartValueName, false);
                    }
                }
            }
            catch
            {
                // 注册表不可写时静默失败
            }
        }

        private static void RefreshAutoStartPath()
        {
            if (IsAutoStartEnabled()) SetAutoStart(true);
        }

        #endregion

        #region 托盘与窗口

        private void SetupTray()
        {
            trayIconHandle = CreateTrayIconHandle();
            tray = new WinForms.NotifyIcon();
            if (trayIconHandle == IntPtr.Zero) return;

            trayDrawingIcon = Drawing.Icon.FromHandle(trayIconHandle);
            tray.Icon = trayDrawingIcon;
            tray.Text = L("下班倒计时", "OffWorkCountdown");
            tray.Visible = true;
            tray.DoubleClick += delegate { ShowWidget(); };

            WinForms.ContextMenuStrip menu = new WinForms.ContextMenuStrip();
            trayToggleItem = new WinForms.ToolStripMenuItem();
            trayToggleItem.Text = L("隐藏窗口", "Hide window");
            trayToggleItem.Click += delegate { ToggleVisibility(); };

            traySettingsItem = new WinForms.ToolStripMenuItem();
            traySettingsItem.Text = L("设置", "Settings");
            traySettingsItem.Click += delegate
            {
                ShowWidget();
                Activate();
                ShowSettings();
            };

            trayExitItem = new WinForms.ToolStripMenuItem();
            trayExitItem.Text = L("退出", "Exit");
            trayExitItem.Click += delegate { ExitApp(); };

            menu.Items.Add(trayToggleItem);
            menu.Items.Add(traySettingsItem);
            menu.Items.Add(new WinForms.ToolStripSeparator());
            menu.Items.Add(trayExitItem);
            tray.ContextMenuStrip = menu;
        }

        private void ShowWidget()
        {
            if (!IsVisible) Show();
            if (dockedHidden && dockEdge != DockEdge.None)
            {
                RevealDock();
            }
            else
            {
                ApplyAnchor();
            }
            UpdateTrayToggleText();
        }

        private void ToggleVisibility()
        {
            if (IsVisible)
            {
                CancelDockHide();
                StopDockSlide();
                StopOffWorkReminder();
                Hide();
            }
            else
            {
                ShowWidget();
            }
            UpdateTrayToggleText();
        }

        private void UpdateTrayToggleText()
        {
            if (trayToggleItem == null) return;
            trayToggleItem.Text = IsVisible ? L("隐藏窗口", "Hide window") : L("显示窗口", "Show window");
        }

        private void ExitApp()
        {
            allowClose = true;
            Close();
            Application.Current.Shutdown();
        }

        private void InitializeAnchor()
        {
            if (settings.WindowRight.HasValue && settings.WindowTop.HasValue)
            {
                anchorRight = settings.WindowRight.Value;
                anchorTop = settings.WindowTop.Value;
                hasCustomPosition = true;
                return;
            }
            Rect area = SystemParameters.WorkArea;
            anchorRight = area.Right;
            anchorTop = area.Top;
            hasCustomPosition = false;
        }

        private void ApplyAnchor()
        {
            if (!anchorReady) return;
            if (dockedHidden && dockEdge != DockEdge.None)
            {
                double hiddenLeft;
                double hiddenTop;
                GetDockPosition(true, out hiddenLeft, out hiddenTop);
                Left = hiddenLeft;
                Top = hiddenTop;
                return;
            }

            double width = ActualWidth > 0 ? ActualWidth : Width;
            if (double.IsNaN(width) || width <= 0) return;
            Left = anchorRight - width;
            Top = anchorTop;
        }

        private void ClampAnchor()
        {
            if (!anchorReady) return;

            double cardWidth = card != null && card.ActualWidth > 0 ? card.ActualWidth : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0 ? card.ActualHeight : surface.Height;
            if (double.IsNaN(cardWidth) || cardWidth <= 0)
            {
                cardWidth = Math.Max(0.0, Width - WindowMargin * 2);
            }
            if (double.IsNaN(cardHeight) || cardHeight <= 0)
            {
                cardHeight = Math.Max(0.0, Height - WindowMargin * 2);
            }
            if (cardWidth <= 0 || cardHeight <= 0) return;

            // 卡片四周留出一个窗口边距，阴影和圆角不会被屏幕边缘/任务栏裁掉。
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            double newRight = Clamp(
                anchorRight,
                work.Left + cardWidth + WindowMargin * 2,
                work.Right);
            double newTop = Clamp(
                anchorTop,
                work.Top,
                work.Bottom - cardHeight - WindowMargin * 2);

            bool moved =
                Math.Abs(newRight - anchorRight) > 0.5 ||
                Math.Abs(newTop - anchorTop) > 0.5;
            anchorRight = newRight;
            anchorTop = newTop;
            if (moved && hasCustomPosition) PersistPosition();
        }

        private Rect WorkAreaForCard(
            double windowRight,
            double windowTop,
            double cardWidth,
            double cardHeight)
        {
            double centerX = windowRight - WindowMargin - cardWidth / 2.0;
            double centerY = windowTop + WindowMargin + cardHeight / 2.0;
            return WorkAreaContaining(centerX, centerY);
        }

        private Rect WorkAreaContaining(double x, double y)
        {
            double scale = 1;
            PresentationSource source = PresentationSource.FromVisual(this);
            if (source != null && source.CompositionTarget != null)
            {
                scale = source.CompositionTarget.TransformToDevice.M11;
                if (scale <= 0) scale = 1;
            }

            System.Drawing.Rectangle work = WinForms.Screen.FromPoint(
                new System.Drawing.Point((int)Math.Round(x * scale), (int)Math.Round(y * scale))).WorkingArea;
            return new Rect(work.Left / scale, work.Top / scale, work.Width / scale, work.Height / scale);
        }

        private Rect ScreenBoundsForCard(
            double windowRight,
            double windowTop,
            double cardWidth,
            double cardHeight)
        {
            double centerX = windowRight - WindowMargin - cardWidth / 2.0;
            double centerY = windowTop + WindowMargin + cardHeight / 2.0;
            return ScreenBoundsContaining(centerX, centerY);
        }

        private Rect ScreenBoundsContaining(double x, double y)
        {
            double scale = 1;
            PresentationSource source = PresentationSource.FromVisual(this);
            if (source != null && source.CompositionTarget != null)
            {
                scale = source.CompositionTarget.TransformToDevice.M11;
                if (scale <= 0) scale = 1;
            }

            System.Drawing.Rectangle bounds = WinForms.Screen.FromPoint(
                new System.Drawing.Point((int)Math.Round(x * scale), (int)Math.Round(y * scale))).Bounds;
            return new Rect(bounds.Left / scale, bounds.Top / scale, bounds.Width / scale, bounds.Height / scale);
        }

        #region 贴边隐藏

        private static double Clamp(double value, double min, double max)
        {
            if (value < min) return min;
            if (value > max) return max;
            return value;
        }

        private DockEdge FindDockEdge(Rect work, Rect screen)
        {
            double cardWidth = card != null && card.ActualWidth > 0
                ? card.ActualWidth
                : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0
                ? card.ActualHeight
                : surface.Height;
            double cardRight = Left + Width - WindowMargin;
            double cardLeft = cardRight - cardWidth;
            double cardTop = Top + WindowMargin;
            double cardBottom = cardTop + cardHeight;

            // 上下优先：卡片落在角落时，优先生效上下隐藏。
            double bestDistance = DockThreshold;
            DockEdge best = DockEdge.None;
            double distance = Top - work.Top;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Top;
            }
            distance = screen.Bottom - cardBottom;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Bottom;
            }
            if (best != DockEdge.None) return best;

            distance = work.Right - cardRight;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Right;
            }
            distance = cardLeft - work.Left;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Left;
            }
            return best;
        }

        private void DockAt(DockEdge edge, Rect work, Rect screen)
        {
            dockEdge = edge;
            double cardWidth = card != null && card.ActualWidth > 0 ? card.ActualWidth : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0 ? card.ActualHeight : surface.Height;

            if (edge == DockEdge.Left || edge == DockEdge.Right)
            {
                anchorTop = Clamp(
                    anchorTop,
                    work.Top - WindowMargin,
                    work.Bottom - cardHeight - WindowMargin);
                anchorRight = edge == DockEdge.Left
                    ? work.Left + cardWidth + WindowMargin
                    : work.Right + WindowMargin;
            }
            else
            {
                anchorRight = Clamp(
                    anchorRight,
                    work.Left + cardWidth + WindowMargin,
                    work.Right + WindowMargin);
                anchorTop = edge == DockEdge.Top
                    ? work.Top - WindowMargin
                    : screen.Bottom - cardHeight - WindowMargin;
            }

            hasCustomPosition = true;
            PersistPosition();
            HideToDock();
        }

        private void RepositionDockedView()
        {
            if (dockEdge == DockEdge.None) return;
            double cardWidth = card != null && card.ActualWidth > 0 ? card.ActualWidth : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0 ? card.ActualHeight : surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            Rect screen = ScreenBoundsForCard(anchorRight, anchorTop, cardWidth, cardHeight);

            if (dockEdge == DockEdge.Left || dockEdge == DockEdge.Right)
            {
                anchorTop = Clamp(
                    anchorTop,
                    work.Top - WindowMargin,
                    work.Bottom - cardHeight - WindowMargin);
                anchorRight = dockEdge == DockEdge.Left
                    ? work.Left + cardWidth + WindowMargin
                    : work.Right + WindowMargin;
            }
            else
            {
                anchorRight = Clamp(
                    anchorRight,
                    work.Left + cardWidth + WindowMargin,
                    work.Right + WindowMargin);
                anchorTop = dockEdge == DockEdge.Top
                    ? work.Top - WindowMargin
                    : screen.Bottom - cardHeight - WindowMargin;
            }

            ApplyAnchor();
            PersistPosition();
        }

        private void GetDockPosition(bool hidden, out double left, out double top)
        {
            double cardWidth = card != null && card.ActualWidth > 0 ? card.ActualWidth : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0 ? card.ActualHeight : surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            Rect screen = ScreenBoundsForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            double right = anchorRight;
            double topAnchor = anchorTop;

            switch (dockEdge)
            {
                case DockEdge.Left:
                    right = hidden
                        ? work.Left + DockSliver + WindowMargin
                        : work.Left + cardWidth + WindowMargin;
                    break;
                case DockEdge.Right:
                    right = hidden
                        ? work.Right - DockSliver + cardWidth + WindowMargin
                        : work.Right + WindowMargin;
                    break;
                case DockEdge.Top:
                    topAnchor = hidden
                        ? work.Top + DockSliver - cardHeight - WindowMargin
                        : work.Top - WindowMargin;
                    break;
                case DockEdge.Bottom:
                    topAnchor = hidden
                        ? screen.Bottom - DockSliver - WindowMargin
                        : screen.Bottom - cardHeight - WindowMargin;
                    break;
            }

            double windowWidth = ActualWidth > 0 ? ActualWidth : Width;
            left = right - windowWidth;
            top = topAnchor;
        }

        private void HideToDock()
        {
            if (dockEdge == DockEdge.None) return;
            dockedHidden = true;
            UpdateDockClip();
            CancelDockHide();
            double left;
            double top;
            GetDockPosition(true, out left, out top);
            StartDockSlide(left, top);
        }

        private void RevealDock()
        {
            if (dockEdge == DockEdge.None) return;
            dockedHidden = false;
            if (surface != null) surface.Clip = null;
            CancelDockHide();
            double left;
            double top;
            GetDockPosition(false, out left, out top);
            StartDockSlide(left, top);
        }

        // 底部隐藏时只保留工作区上沿的 6px 把手，卡片其余部分不绘制，
        // 避免置顶窗口的卡片遮住 Windows 任务栏。
        private void UpdateDockClip()
        {
            if (surface == null) return;
            if (dockedHidden && dockEdge == DockEdge.Bottom)
            {
                surface.Clip = new RectangleGeometry(
                    new Rect(0, 0, Math.Max(0, surface.Width), DockSliver));
            }
            else
            {
                surface.Clip = null;
            }
        }

        private void StartDockSlide(double targetLeft, double targetTop)
        {
            if (dockSlideTimer == null) return;
            if (Math.Abs(Left - targetLeft) < 0.5 && Math.Abs(Top - targetTop) < 0.5)
            {
                Left = targetLeft;
                Top = targetTop;
                dockSlideTimer.Stop();
                return;
            }

            dockSlideFromLeft = Left;
            dockSlideFromTop = Top;
            dockSlideToLeft = targetLeft;
            dockSlideToTop = targetTop;
            dockSlideStarted = DateTime.UtcNow;
            dockSlideTimer.Stop();
            dockSlideTimer.Start();
        }

        private void OnDockSlideTick(object sender, EventArgs e)
        {
            double elapsed = (DateTime.UtcNow - dockSlideStarted).TotalMilliseconds;
            double progress = elapsed / DockSlideMs;
            if (progress < 0) progress = 0;
            if (progress > 1) progress = 1;
            double eased = 1.0 - Math.Pow(1.0 - progress, 3.0);
            Left = dockSlideFromLeft + (dockSlideToLeft - dockSlideFromLeft) * eased;
            Top = dockSlideFromTop + (dockSlideToTop - dockSlideFromTop) * eased;
            if (progress >= 1.0)
            {
                Left = dockSlideToLeft;
                Top = dockSlideToTop;
                dockSlideTimer.Stop();
            }
        }

        private void StopDockSlide()
        {
            if (dockSlideTimer != null) dockSlideTimer.Stop();
        }

        private void ScheduleDockHide()
        {
            if (dockEdge == DockEdge.None || dockedHidden || settingsOpen || dragInProgress) return;
            if (offWorkRevealUntil.HasValue) return;
            if (card != null && card.ContextMenu != null && card.ContextMenu.IsOpen) return;
            if (dockHideTimer == null) return;
            dockHideTimer.Stop();
            dockHideTimer.Start();
        }

        private void CancelDockHide()
        {
            if (dockHideTimer != null) dockHideTimer.Stop();
        }

        private void OnDockHideTick(object sender, EventArgs e)
        {
            dockHideTimer.Stop();
            if (dockEdge == DockEdge.None || dockedHidden || settingsOpen || dragInProgress) return;
            if (offWorkRevealUntil.HasValue) return;
            if (card != null && card.ContextMenu != null && card.ContextMenu.IsOpen) return;
            if (card != null && card.IsMouseOver) return;
            HideToDock();
        }

        private void OnCardMouseEnter(object sender, MouseEventArgs e)
        {
            CancelDockHide();
            if (dockEdge != DockEdge.None && dockedHidden)
            {
                RevealDock();
            }
        }

        private void OnCardMouseLeave(object sender, MouseEventArgs e)
        {
            ScheduleDockHide();
        }

        #endregion

        private void PersistPosition()
        {
            settings.WindowRight = anchorRight;
            settings.WindowTop = anchorTop;
            SettingsStore.Save(settings);
        }

        private void OnDragStart(object sender, MouseButtonEventArgs e)
        {
            if (dragInProgress || e.ChangedButton != MouseButton.Left) return;
            CancelDockHide();
            bool fromCollapsed =
                collapsedView.Visibility == Visibility.Visible &&
                IsWithinCollapsed(e.OriginalSource);
            if (!fromCollapsed && !CanStartDrag(e.OriginalSource)) return;

            dragCandidate = true;
            dragFromCollapsed = fromCollapsed;
            dragStartPoint = e.GetPosition(this);
            // 折叠态本身是 Button，这里接管鼠标，避免按压缩放阻止窗口拖动
            if (fromCollapsed) e.Handled = true;
        }

        private void OnDragMove(object sender, MouseEventArgs e)
        {
            if (!dragCandidate || e.LeftButton != MouseButtonState.Pressed) return;
            Point current = e.GetPosition(this);
            double dx = Math.Abs(current.X - dragStartPoint.X);
            double dy = Math.Abs(current.Y - dragStartPoint.Y);
            if (dx < SystemParameters.MinimumHorizontalDragDistance &&
                dy < SystemParameters.MinimumVerticalDragDistance)
            {
                return;
            }

            bool wasCollapsed = dragFromCollapsed;
            dragCandidate = false;
            dragFromCollapsed = false;
            dragInProgress = true;
            CancelDockHide();
            StopDockSlide();
            StopOffWorkReminder();
            // 一旦开始拖动就先离开贴边状态，松手后再重新判断落在哪一侧。
            dockEdge = DockEdge.None;
            dockedHidden = false;
            if (surface != null) surface.Clip = null;
            if (wasCollapsed) Mouse.Capture(null);
            try
            {
                DragMove();
            }
            catch (InvalidOperationException)
            {
                // 鼠标在这之前已经松开时，不进入系统拖动循环
            }
            dragInProgress = false;

            anchorRight = Left + ActualWidth;
            anchorTop = Top;
            hasCustomPosition = true;

            double cardWidth = card != null && card.ActualWidth > 0 ? card.ActualWidth : surface.Width;
            double cardHeight = card != null && card.ActualHeight > 0 ? card.ActualHeight : surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            Rect screen = ScreenBoundsForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            DockEdge edge = FindDockEdge(work, screen);
            if (edge != DockEdge.None)
            {
                DockAt(edge, work, screen);
            }
            else
            {
                ClampAnchor();
                ApplyAnchor();
                PersistPosition();
            }
        }

        private void OnDragEnd(object sender, MouseButtonEventArgs e)
        {
            if (!dragCandidate) return;
            bool expandCollapsed = dragFromCollapsed && collapsedView.Visibility == Visibility.Visible;
            dragCandidate = false;
            dragFromCollapsed = false;
            if (expandCollapsed)
            {
                Activate();
                ShowCountdown();
            }
        }

        private bool IsWithinCollapsed(object originalSource)
        {
            DependencyObject node = originalSource as DependencyObject;
            while (node != null && node != surface)
            {
                if (node == collapsedView) return true;
                node = VisualTreeHelper.GetParent(node);
            }
            return false;
        }

        private bool CanStartDrag(object originalSource)
        {
            DependencyObject node = originalSource as DependencyObject;
            while (node != null && node != surface)
            {
                if (node is ButtonBase || node is TextBoxBase || node is PasswordBox) return false;
                node = VisualTreeHelper.GetParent(node);
            }
            return true;
        }

        private void ShowCountdown()
        {
            settingsOpen = false;
            RefreshUi();
            SwitchView(countdownView, ExpandedWidth, countdownCardHeight);
        }

        private void ShowSettings()
        {
            settingsOpen = true;
            suppressSettingsApply = true;
            if (soundToggle != null) soundToggle.IsChecked = settings.SoundEnabled;
            if (autoStartToggle != null) autoStartToggle.IsChecked = IsAutoStartEnabled();
            startBox.Text = settings.StartTime;
            endBox.Text = settings.EndTime;
            suppressSettingsApply = false;
            UpdateLanguageSegments();
            UpdateOvernightBadge();
            ResetSettingsMessage();
            if (currentView == settingsView)
            {
                return;
            }
            SwitchView(settingsView, ExpandedWidth, settingsCardHeight);
        }

        private void ApplyLanguage()
        {
            FontFamily = IsEnglish ? PickEnglishFont() : PickChineseFont();
            countdownText.FontFamily = PickEnglishFont();
            collapsedTime.FontFamily = PickEnglishFont();
            startBox.FontFamily = PickEnglishFont();
            endBox.FontFamily = PickEnglishFont();
            footerTimeRun.FontFamily = PickEnglishFont();
            footerRightText.FontFamily = PickEnglishFont();

            doneText.Text = L("今天辛苦了。", "Good work today.");

            ToolTipService.SetToolTip(settingsButton, L("设置", "Settings"));
            AutomationProperties.SetName(settingsButton, L("设置", "Settings"));
            ToolTipService.SetToolTip(collapseButton, L("收起", "Collapse"));
            AutomationProperties.SetName(collapseButton, L("收起", "Collapse"));

            ToolTipService.SetToolTip(
                collapsedView,
                L("拖动调整位置，点击展开", "Drag to move, click to expand"));
            AutomationProperties.SetName(
                collapsedView,
                L("展开下班倒计时", "Expand countdown"));

            settingsTitle.Text = L("倒计时设置", "Countdown settings");
            startFieldLabel.Text = L("上班时间", "Start time");
            endFieldLabel.Text = L("下班时间", "End time");
            AutomationProperties.SetName(startBox, L("上班时间", "Start time"));
            AutomationProperties.SetName(endBox, L("下班时间", "End time"));
            soundLabel.Text = L("下班提醒音", "End-of-day sound");
            AutomationProperties.SetName(
                soundToggle,
                L("开启下班提醒音", "Enable end-of-day sound"));
            autoStartLabel.Text = L("开机自启", "Start with Windows");
            AutomationProperties.SetName(
                autoStartToggle,
                L("开机自启开关", "Start with Windows toggle"));
            languageLabel.Text = L("语言", "Language");
            if (backgroundLabel != null)
            {
                backgroundLabel.Text = L("背景", "Background");
                backgroundNoneButton.Content = L("无背景", "None");
                backgroundAuroraButton.Content = L("极光", "Aurora");
                backgroundOceanButton.Content = L("大海", "Ocean");
                UpdateBackdropSegments();
            }
            UpdateLanguageSegments();
            UpdateOvernightBadge();
            ToolTipService.SetToolTip(closeSettingsButton, L("关闭设置", "Close settings"));
            AutomationProperties.SetName(closeSettingsButton, L("关闭设置", "Close settings"));

            contextSettingsItem.Header = L("设置", "Settings");
            contextHideItem.Header = L("隐藏窗口", "Hide window");
            contextExitItem.Header = L("退出", "Exit");
            if (traySettingsItem != null) traySettingsItem.Text = L("设置", "Settings");
            if (trayExitItem != null) trayExitItem.Text = L("退出", "Exit");
            UpdateTrayToggleText();

            MeasureCardHeights();
            double maxCardHeight = Math.Max(
                countdownCardHeight,
                Math.Max(settingsCardHeight, collapsedCardHeight));
            Width = ExpandedWidth + WindowMargin * 2;
            Height = maxCardHeight + WindowMargin * 2 + SettingsErrorAllowance;
            surface.Width = currentView == collapsedView ? CollapsedWidth : ExpandedWidth;
            surface.Height = currentView == settingsView
                ? settingsCardHeight
                : (currentView == collapsedView ? collapsedCardHeight : countdownCardHeight);
            if (dockEdge != DockEdge.None)
            {
                StopDockSlide();
                RepositionDockedView();
            }
            else
            {
                ClampAnchor();
                ApplyAnchor();
            }

            RefreshUi();
        }

        private void Collapse()
        {
            settingsOpen = false;
            RefreshUi();
            SwitchView(collapsedView, CollapsedWidth, collapsedCardHeight);
        }

        private void MeasureCardHeights()
        {
            countdownCardHeight = MeasureCardHeight(countdownView, ExpandedWidth);
            settingsCardHeight = MeasureCardHeight(settingsView, ExpandedWidth);
            collapsedCardHeight = MeasureCardHeight(collapsedView, CollapsedWidth);
        }

        private double MeasureCardHeight(FrameworkElement view, double cardWidth)
        {
            Visibility previous = view.Visibility;
            view.Visibility = Visibility.Hidden;
            view.InvalidateMeasure();
            view.Measure(new Size(cardWidth - 2, double.PositiveInfinity));
            double height = Math.Ceiling(view.DesiredSize.Height) + 2;
            view.Visibility = previous;
            return height;
        }

        private static TranslateTransform EnsureShift(FrameworkElement view)
        {
            TranslateTransform shift = view.RenderTransform as TranslateTransform;
            if (shift == null)
            {
                shift = new TranslateTransform();
                view.RenderTransform = shift;
            }
            return shift;
        }

        private static void AnimateView(
            FrameworkElement view,
            double fromOpacity,
            double toOpacity,
            double fromY,
            double toY,
            int milliseconds,
            Action completed)
        {
            TranslateTransform shift = EnsureShift(view);
            view.Opacity = toOpacity;
            DoubleAnimation fade = new DoubleAnimation(
                fromOpacity,
                toOpacity,
                TimeSpan.FromMilliseconds(milliseconds));
            fade.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            if (completed != null)
            {
                fade.Completed += delegate { completed(); };
            }
            view.BeginAnimation(UIElement.OpacityProperty, fade);

            DoubleAnimation slide = new DoubleAnimation(
                fromY,
                toY,
                TimeSpan.FromMilliseconds(milliseconds));
            slide.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            shift.BeginAnimation(TranslateTransform.YProperty, slide);
        }

        private void AnimateSurfaceSize(
            double fromWidth,
            double toWidth,
            double fromHeight,
            double toHeight)
        {
            if (!SystemParameters.ClientAreaAnimation) return;

            DoubleAnimation width = new DoubleAnimation(
                fromWidth,
                toWidth,
                TimeSpan.FromMilliseconds(230));
            width.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            surface.BeginAnimation(FrameworkElement.WidthProperty, width);

            DoubleAnimation height = new DoubleAnimation(
                fromHeight,
                toHeight,
                TimeSpan.FromMilliseconds(230));
            height.EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut };
            surface.BeginAnimation(FrameworkElement.HeightProperty, height);
        }

        /// <summary>
        /// 在固定尺寸的顶层窗口内做卡片尺寸/内容过渡，窗口本身不改变大小，避免闪字。
        /// </summary>
        private void SwitchView(FrameworkElement target, double targetWidth, double targetHeight)
        {
            if (target == null) return;
            if (currentView == target &&
                Math.Abs(surface.Width - targetWidth) < 0.5 &&
                Math.Abs(surface.Height - targetHeight) < 0.5)
            {
                return;
            }

            FrameworkElement previous = currentView;
            currentView = target;

            double fromWidth = surface.Width;
            double fromHeight = surface.Height;
            surface.Width = targetWidth;
            surface.Height = targetHeight;
            AnimateSurfaceSize(fromWidth, targetWidth, fromHeight, targetHeight);

            if (previous != null && previous != target)
            {
                previous.IsHitTestVisible = false;
                AnimateView(previous, previous.Opacity, 0, 0, -6, 170, null);
            }

            target.Visibility = Visibility.Visible;
            target.IsHitTestVisible = true;
            target.Opacity = 0;
            EnsureShift(target).Y = 7;
            AnimateView(target, 0, 1, 7, 0, 230, delegate
            {
                if (previous != null && previous != target && previous != currentView)
                {
                    previous.Visibility = Visibility.Collapsed;
                    previous.Opacity = 0;
                }
            });

            if (dockEdge != DockEdge.None)
            {
                StopDockSlide();
                RepositionDockedView();
            }
            else
            {
                ClampAnchor();
                ApplyAnchor();
            }
            UpdateProgressWidth();
            ScheduleDockHide();
        }
        private void OnSettingsKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Enter)
            {
                NormalizeTimeBox(startBox);
                NormalizeTimeBox(endBox);
                ApplyTimeSettings();
                e.Handled = true;
            }
            else if (e.Key == Key.Escape)
            {
                ShowCountdown();
                e.Handled = true;
            }
        }

        private void ApplyTimeSettings()
        {
            if (!settingsOpen) return;

            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            if (!CountdownSettings.TryParseFlexibleTime(startBox.Text, out startText, out start))
            {
                ShowSettingsError(L(
                    "上班时间格式不对",
                    "Invalid start time"));
                startBox.Focus();
                startBox.SelectAll();
                return;
            }
            if (!CountdownSettings.TryParseFlexibleTime(endBox.Text, out endText, out end))
            {
                ShowSettingsError(L(
                    "下班时间格式不对",
                    "Invalid end time"));
                endBox.Focus();
                endBox.SelectAll();
                return;
            }


            startBox.Text = startText;
            endBox.Text = endText;
            settings.StartTime = startText;
            settings.EndTime = endText;
            workStart = start;
            workEnd = end;
            SettingsStore.Save(settings);
            ResetSettingsMessage();
            RefreshUi();
        }

        private void ApplyStepperTime(bool endField)
        {
            if (!settingsOpen) return;

            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;

            if (endField)
            {
                if (!CountdownSettings.TryParseFlexibleTime(endBox.Text, out endText, out end))
                {
                    ShowSettingsError(L("下班时间格式不对", "Invalid end time"));
                    endBox.Focus();
                    endBox.SelectAll();
                    return;
                }
                if (!CountdownSettings.TryParseFlexibleTime(startBox.Text, out startText, out start))
                {
                    startText = settings.StartTime;
                    if (!CountdownSettings.TryParseFlexibleTime(startText, out startText, out start))
                    {
                        startText = CountdownSettings.DefaultStart;
                        CountdownSettings.TryParseFlexibleTime(startText, out startText, out start);
                    }
                    startBox.Text = startText;
                }
            }
            else
            {
                if (!CountdownSettings.TryParseFlexibleTime(startBox.Text, out startText, out start))
                {
                    ShowSettingsError(L("上班时间格式不对", "Invalid start time"));
                    startBox.Focus();
                    startBox.SelectAll();
                    return;
                }
                if (!CountdownSettings.TryParseFlexibleTime(endBox.Text, out endText, out end))
                {
                    endText = settings.EndTime;
                    if (!CountdownSettings.TryParseFlexibleTime(endText, out endText, out end))
                    {
                        endText = CountdownSettings.DefaultEnd;
                        CountdownSettings.TryParseFlexibleTime(endText, out endText, out end);
                    }
                    endBox.Text = endText;
                }
            }



            startBox.Text = startText;
            endBox.Text = endText;
            settings.StartTime = startText;
            settings.EndTime = endText;
            workStart = start;
            workEnd = end;
            SettingsStore.Save(settings);
            ResetSettingsMessage();
            RefreshUi();
        }

        private void ShowSettingsError(string message)
        {
            bool wasVisible = settingsError.Visibility == Visibility.Visible;
            settingsError.Text = message;
            settingsError.Foreground = Danger;
            settingsError.MinHeight = 16;
            settingsError.Visibility = Visibility.Visible;
            if (!wasVisible) UpdateSettingsCardHeight();
        }

        private void ResetSettingsMessage()
        {
            if (settingsError == null) return;
            bool wasVisible = settingsError.Visibility == Visibility.Visible;
            settingsError.Text = string.Empty;
            settingsError.Foreground = TextDisabled;
            settingsError.MinHeight = 0;
            settingsError.Visibility = Visibility.Collapsed;
            if (wasVisible) UpdateSettingsCardHeight();
        }

        private void UpdateSettingsCardHeight()
        {
            if (settingsView == null) return;
            settingsCardHeight = MeasureCardHeight(settingsView, ExpandedWidth);
            double maxCardHeight = Math.Max(
                countdownCardHeight,
                Math.Max(settingsCardHeight, collapsedCardHeight));
            double targetHeight = maxCardHeight + WindowMargin * 2 + SettingsErrorAllowance;
            if (Math.Abs(Height - targetHeight) > 0.5) Height = targetHeight;
            if (currentView == settingsView)
            {
                surface.Height = settingsCardHeight;
            }
            if (dockEdge != DockEdge.None)
            {
                StopDockSlide();
                RepositionDockedView();
            }
            else
            {
                ClampAnchor();
                ApplyAnchor();
            }
        }

        private void RefreshTimeSpans()
        {
            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            if (!CountdownSettings.TryNormalizeTime(settings.StartTime, out startText, out start) ||
                !CountdownSettings.TryNormalizeTime(settings.EndTime, out endText, out end))
            {
                settings.StartTime = CountdownSettings.DefaultStart;
                settings.EndTime = CountdownSettings.DefaultEnd;
                start = TimeSpan.FromHours(9);
                end = TimeSpan.FromHours(18.5);
            }
            workStart = start;
            workEnd = end;
        }

        private void OnWorkEnded()
        {
            if (settings.SoundEnabled) PlayChime();

            if (!IsVisible) return;
            if (dockEdge == DockEdge.None || !dockedHidden) return;
            if (offWorkRevealUntil.HasValue) return;

            // 贴边隐藏时自动滑出 5 秒提醒下班，然后缩回。
            offWorkRevealUntil = DateTime.Now.AddSeconds(5);
            CancelDockHide();
            RevealDock();
        }

        private void StopOffWorkReminder()
        {
            offWorkRevealUntil = null;
        }

        private void PlayChime()
        {
            try
            {
                if (chimePlayer == null)
                {
                    chimeStream = BuildChimeStream();
                    chimePlayer = new SoundPlayer(chimeStream);
                    chimePlayer.Load();
                }
                chimePlayer.Play();
            }
            catch
            {
                // 没有音频设备时静默失败
            }
        }

        private static Stream BuildChimeStream()
        {
            const int sampleRate = 44100;
            const double seconds = 0.85;
            int sampleCount = (int)(sampleRate * seconds);
            double[] values = new double[sampleCount];
            double[] frequencies = new double[] { 880.0, 1174.66 };
            double[] starts = new double[] { 0.0, 0.16 };
            double[] amplitudes = new double[] { 0.62, 0.52 };

            double peak = 0.0;
            for (int i = 0; i < sampleCount; i++)
            {
                double time = (double)i / sampleRate;
                double value = 0.0;
                for (int n = 0; n < frequencies.Length; n++)
                {
                    double local = time - starts[n];
                    if (local < 0) continue;

                    double attack = Math.Min(1.0, local / 0.004);
                    double decay = Math.Exp(-6.5 * local);
                    value += amplitudes[n] * attack * decay * (
                        Math.Sin(2.0 * Math.PI * frequencies[n] * local) +
                        0.28 * Math.Exp(-3.5 * local) *
                            Math.Sin(2.0 * Math.PI * frequencies[n] * 2.01 * local));
                }
                values[i] = value;
                if (Math.Abs(value) > peak) peak = Math.Abs(value);
            }

            double scale = peak > 0.0 ? 0.72 / peak : 1.0;
            MemoryStream stream = new MemoryStream();
            BinaryWriter writer = new BinaryWriter(stream);

            int dataSize = sampleCount * 2;
            writer.Write(Encoding.ASCII.GetBytes("RIFF"));
            writer.Write(36 + dataSize);
            writer.Write(Encoding.ASCII.GetBytes("WAVE"));
            writer.Write(Encoding.ASCII.GetBytes("fmt "));
            writer.Write(16);
            writer.Write((short)1);
            writer.Write((short)1);
            writer.Write(sampleRate);
            writer.Write(sampleRate * 2);
            writer.Write((short)2);
            writer.Write((short)16);
            writer.Write(Encoding.ASCII.GetBytes("data"));
            writer.Write(dataSize);
            for (int i = 0; i < sampleCount; i++)
            {
                double sample = values[i] * scale;
                if (sample > 1.0) sample = 1.0;
                if (sample < -1.0) sample = -1.0;
                writer.Write((short)Math.Round(sample * 32767.0));
            }

            writer.Flush();
            stream.Position = 0;
            return stream;
        }

        #endregion

        #region 刷新与绘制

        private void RefreshUi()
        {
            DateTime now = DateTime.Now;
            bool overnight = workEnd <= workStart;
            DateTime todayStart = now.Date.Add(workStart);
            DateTime todayEnd = now.Date.Add(workEnd);
            DateTime startAt;
            DateTime endAt;
            bool working;
            if (overnight)
            {
                // 跨天班次：当天 00:00 到下班时间之间仍属于前一天开始的班次；
                // 下班之后到当天上班前，显示下一班的倒计时（不能提前显示已下班）。
                if (now < todayEnd)
                {
                    startAt = todayStart.AddDays(-1);
                    endAt = todayEnd;
                }
                else
                {
                    startAt = todayStart;
                    endAt = todayEnd.AddDays(1);
                }
                working = true;
            }
            else
            {
                startAt = todayStart;
                endAt = todayEnd;
                working = now < endAt;
            }

            bool previousWorking = lastWorking.HasValue ? lastWorking.Value : working;
            bool workJustEnded = previousWorking && !working;

            TimeSpan remaining = working ? endAt - now : TimeSpan.Zero;
            string countdown = FormatCountdown(remaining);
            int percent = 100;
            if (working)
            {
                double total = (endAt - startAt).TotalMilliseconds;
                double done = (now - startAt).TotalMilliseconds;
                if (done < 0) done = 0;
                if (done > total) done = total;
                percent = (int)Math.Round(
                    (done / total) * 100.0,
                    MidpointRounding.AwayFromZero);
            }
            currentPercent = percent;

            bool workStateChanged = lastWorking.HasValue && lastWorking.Value != working;
            bool darkBackdrop = backdropMode != "none";
            Brush workingAccent = darkBackdrop ? (Brush)Brushes.White : TextSecondary;
            Brush offAccent = darkBackdrop ? (Brush)AuroraDone : Success;
            if (lastWorking == null || lastWorking.Value != working)
            {
                double headerSize = darkBackdrop ? 18.0 : 16.0;
                headerIcon.Content = working
                    ? (object)CreateClockIcon(headerSize, workingAccent)
                    : (object)CreateCheckIcon(headerSize, offAccent);
                collapsedIcon.Content = working
                    ? (object)CreateClockIcon(16.0, workingAccent)
                    : (object)CreateCheckIcon(16.0, offAccent);
                lastWorking = working;
            }

            statusLabel.Text = working ? L("距离下班", "Until off work") : L("已下班", "Off work");
            statusLabel.Foreground = working ? workingAccent : offAccent;
            AnimateWorkText(working, workStateChanged);
            if (countdownText.Text != countdown) countdownText.Text = countdown;
            footerCaptionRun.Text = working
                ? L("下班时间 ", "Off at ")
                : L("明天 ", "Tomorrow ");
            footerTimeRun.Text = settings.EndTime;
            footerRightText.Text = percent + "%";
            progressFill.Background = working ? ProgressFill : Success;

            collapsedLabel.Text = working ? L("距离下班", "Time left") : L("已下班", "Off work");
            if (AuroraEffect.IsAvailable)
            {
                collapsedLabel.Foreground = working ? AuroraTextSoft : AuroraDone;
                collapsedTime.Text = working ? countdown : L("明天见", "Tomorrow");
                collapsedTime.Foreground = working ? AuroraTextBright : AuroraDone;
            }
            else
            {
                collapsedLabel.Foreground = working ? TextSecondary : Success;
                collapsedTime.Text = working ? countdown : L("明天见", "Tomorrow");
                collapsedTime.Foreground = working ? TextPrimary : Success;
            }

            UpdateProgressWidth();

            if (tray != null)
            {
                string tip = working
                    ? L("距下班 ", "Time left ") + countdown
                    : L("已下班，明天 ", "Off work, tomorrow ") + settings.EndTime;
                if (tray.Text != tip) tray.Text = tip;
            }

            if (workJustEnded)
            {
                DateTime endMoment = now.Date.Add(workEnd);
                if (lastReminderEnd != endMoment)
                {
                    lastReminderEnd = endMoment;
                    OnWorkEnded();
                }
            }

            if (offWorkRevealUntil.HasValue && now >= offWorkRevealUntil.Value)
            {
                offWorkRevealUntil = null;
                if (IsVisible &&
                    dockEdge != DockEdge.None &&
                    !dockedHidden &&
                    !settingsOpen &&
                    !dragInProgress &&
                    (card == null || !card.IsMouseOver))
                {
                    HideToDock();
                }
            }
        }

        private void AnimateWorkText(bool working, bool animate)
        {
            FrameworkElement incoming = working ? (FrameworkElement)countdownText : doneText;
            FrameworkElement outgoing = working ? (FrameworkElement)doneText : countdownText;

            if (!animate || !SystemParameters.ClientAreaAnimation)
            {
                incoming.Visibility = Visibility.Visible;
                incoming.Opacity = 1;
                EnsureShift(incoming).Y = 0;
                outgoing.Visibility = Visibility.Collapsed;
                outgoing.Opacity = 0;
                return;
            }

            incoming.Visibility = Visibility.Visible;
            incoming.Opacity = 0;
            EnsureShift(incoming).Y = 8;
            AnimateView(incoming, 0, 1, 8, 0, 260, delegate
            {
                if (lastWorking.HasValue && lastWorking.Value == working)
                {
                    outgoing.Visibility = Visibility.Collapsed;
                    outgoing.Opacity = 0;
                }
            });

            if (outgoing.Visibility == Visibility.Visible)
            {
                AnimateView(outgoing, 1, 0, 0, -8, 200, null);
            }
        }
        private void UpdateProgressWidth()
        {
            if (progressFill == null || progressTrack == null) return;
            double width = progressTrack.ActualWidth;
            if (width <= 0)
            {
                progressFill.Width = 0;
                return;
            }
            progressFill.Width = width * currentPercent / 100.0;
        }

        private static string FormatCountdown(TimeSpan remaining)
        {
            int total = (int)Math.Ceiling(remaining.TotalSeconds);
            if (total < 0) total = 0;
            int hours = total / 3600;
            int minutes = (total % 3600) / 60;
            int seconds = total % 60;
            return string.Format(
                CultureInfo.InvariantCulture,
                "{0:00}:{1:00}:{2:00}",
                hours,
                minutes,
                seconds);
        }

        private static Viewbox CreateClockIcon(double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 16;
            canvas.Height = 16;

            Ellipse circle = new Ellipse();
            circle.Width = 13;
            circle.Height = 13;
            circle.Stroke = stroke;
            circle.StrokeThickness = 1.3;
            Canvas.SetLeft(circle, 1.5);
            Canvas.SetTop(circle, 1.5);

            Line hourHand = new Line();
            hourHand.X1 = 8;
            hourHand.Y1 = 4.6;
            hourHand.X2 = 8;
            hourHand.Y2 = 8.4;
            hourHand.Stroke = stroke;
            hourHand.StrokeThickness = 1.3;
            hourHand.StrokeStartLineCap = PenLineCap.Round;
            hourHand.StrokeEndLineCap = PenLineCap.Round;

            Line minuteHand = new Line();
            minuteHand.X1 = 8;
            minuteHand.Y1 = 8.4;
            minuteHand.X2 = 11.1;
            minuteHand.Y2 = 9.8;
            minuteHand.Stroke = stroke;
            minuteHand.StrokeThickness = 1.3;
            minuteHand.StrokeStartLineCap = PenLineCap.Round;
            minuteHand.StrokeEndLineCap = PenLineCap.Round;

            canvas.Children.Add(circle);
            canvas.Children.Add(hourHand);
            canvas.Children.Add(minuteHand);

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static Viewbox CreateSlidersIcon(double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 16;
            canvas.Height = 16;
            AddSlider(canvas, 5.2, 10.4, stroke);
            AddSlider(canvas, 10.8, 5.6, stroke);

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static FrameworkElement CreateSettingsIcon(Brush brush)
        {
            FontFamily iconFont = FindIconFont();
            if (iconFont == null) return CreateSlidersIcon(16, brush);

            TextBlock glyph = new TextBlock();
            glyph.Text = "\uE713";
            glyph.FontFamily = iconFont;
            glyph.FontSize = 16;
            glyph.Foreground = brush;
            glyph.TextAlignment = TextAlignment.Center;

            Viewbox box = new Viewbox();
            box.Width = 16;
            box.Height = 16;
            box.Stretch = Stretch.Uniform;
            box.Child = glyph;
            return box;
        }

        private static FontFamily FindIconFont()
        {
            foreach (FontFamily family in Fonts.SystemFontFamilies)
            {
                if (string.Equals(
                        family.Source,
                        "Segoe Fluent Icons",
                        StringComparison.OrdinalIgnoreCase))
                {
                    return family;
                }
            }
            foreach (FontFamily family in Fonts.SystemFontFamilies)
            {
                if (string.Equals(
                        family.Source,
                        "Segoe MDL2 Assets",
                        StringComparison.OrdinalIgnoreCase))
                {
                    return family;
                }
            }
            return null;
        }

        private static void AddSlider(Canvas canvas, double y, double knobX, Brush stroke)
        {
            Line rail = new Line();
            rail.X1 = 2.5;
            rail.Y1 = y;
            rail.X2 = 13.5;
            rail.Y2 = y;
            rail.Stroke = stroke;
            rail.StrokeThickness = 1.3;
            rail.StrokeStartLineCap = PenLineCap.Round;
            rail.StrokeEndLineCap = PenLineCap.Round;
            canvas.Children.Add(rail);

            Ellipse knob = new Ellipse();
            knob.Width = 3.2;
            knob.Height = 3.2;
            knob.Fill = Surface;
            knob.Stroke = stroke;
            knob.StrokeThickness = 1.2;
            Canvas.SetLeft(knob, knobX - 1.6);
            Canvas.SetTop(knob, y - 1.6);
            canvas.Children.Add(knob);
        }

        private static Viewbox CreateMinusIcon(double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 16;
            canvas.Height = 16;

            Line line = new Line();
            line.X1 = 3.2;
            line.Y1 = 8;
            line.X2 = 12.8;
            line.Y2 = 8;
            line.Stroke = stroke;
            line.StrokeThickness = 1.3;
            line.StrokeStartLineCap = PenLineCap.Round;
            line.StrokeEndLineCap = PenLineCap.Round;
            canvas.Children.Add(line);

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static Viewbox CreateCloseIcon(double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 16;
            canvas.Height = 16;

            Line first = new Line();
            first.X1 = 4.2;
            first.Y1 = 4.2;
            first.X2 = 11.8;
            first.Y2 = 11.8;
            first.Stroke = stroke;
            first.StrokeThickness = 1.3;
            first.StrokeStartLineCap = PenLineCap.Round;
            first.StrokeEndLineCap = PenLineCap.Round;

            Line second = new Line();
            second.X1 = 11.8;
            second.Y1 = 4.2;
            second.X2 = 4.2;
            second.Y2 = 11.8;
            second.Stroke = stroke;
            second.StrokeThickness = 1.3;
            second.StrokeStartLineCap = PenLineCap.Round;
            second.StrokeEndLineCap = PenLineCap.Round;

            canvas.Children.Add(first);
            canvas.Children.Add(second);

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static Viewbox CreateCheckIcon(double size, Brush stroke)
        {
            Canvas canvas = new Canvas();
            canvas.Width = 16;
            canvas.Height = 16;

            Ellipse circle = new Ellipse();
            circle.Width = 13;
            circle.Height = 13;
            circle.Stroke = stroke;
            circle.StrokeThickness = 1.3;
            Canvas.SetLeft(circle, 1.5);
            Canvas.SetTop(circle, 1.5);

            System.Windows.Shapes.Path tick = new System.Windows.Shapes.Path();
            tick.Data = Geometry.Parse("M 4.3 8.2 L 6.7 10.6 L 11.8 5.2");
            tick.Stroke = stroke;
            tick.StrokeThickness = 1.5;
            tick.StrokeStartLineCap = PenLineCap.Round;
            tick.StrokeEndLineCap = PenLineCap.Round;
            tick.StrokeLineJoin = PenLineJoin.Round;
            canvas.Children.Add(circle);
            canvas.Children.Add(tick);

            Viewbox box = new Viewbox();
            box.Width = size;
            box.Height = size;
            box.Stretch = Stretch.Uniform;
            box.Child = canvas;
            return box;
        }

        private static SolidColorBrush MakeBrush(byte r, byte g, byte b)
        {
            SolidColorBrush brush = new SolidColorBrush(Color.FromRgb(r, g, b));
            brush.Freeze();
            return brush;
        }

        private static Brush MakeCardFill()
        {
            LinearGradientBrush brush = new LinearGradientBrush();
            brush.StartPoint = new Point(0, 0);
            brush.EndPoint = new Point(1, 1);
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xFD, 0xFE, 0xFF), 0.0));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xF6, 0xFA, 0xFE), 0.55));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xEF, 0xF5, 0xFC), 1.0));
            brush.Freeze();
            return brush;
        }

        private static Brush MakeCardBorder()
        {
            LinearGradientBrush brush = new LinearGradientBrush();
            brush.StartPoint = new Point(0, 0);
            brush.EndPoint = new Point(1, 1);
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xFF, 0xFF, 0xFF), 0.0));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xE2, 0xE9, 0xF2), 0.5));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0xD3, 0xDF, 0xEE), 1.0));
            brush.Freeze();
            return brush;
        }

        private static Brush MakeProgressFill()
        {
            LinearGradientBrush brush = new LinearGradientBrush();
            brush.StartPoint = new Point(0, 0);
            brush.EndPoint = new Point(1, 0);
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0x0F, 0x6C, 0xBD), 0.0));
            brush.GradientStops.Add(new GradientStop(Color.FromRgb(0x3B, 0x8D, 0xE0), 1.0));
            brush.Freeze();
            return brush;
        }

        private static Brush MakeSheenFill()
        {
            LinearGradientBrush brush = new LinearGradientBrush();
            brush.StartPoint = new Point(0, 0);
            brush.EndPoint = new Point(0, 1);
            brush.GradientStops.Add(new GradientStop(Color.FromArgb(44, 0xFF, 0xFF, 0xFF), 0.0));
            brush.GradientStops.Add(new GradientStop(Color.FromArgb(0, 0xFF, 0xFF, 0xFF), 1.0));
            brush.Freeze();
            return brush;
        }

        private static Brush MakeSpecularFill()
        {
            LinearGradientBrush brush = new LinearGradientBrush();
            brush.StartPoint = new Point(0, 0);
            brush.EndPoint = new Point(1, 0);
            brush.GradientStops.Add(new GradientStop(Color.FromArgb(0, 0xFF, 0xFF, 0xFF), 0.0));
            brush.GradientStops.Add(new GradientStop(Color.FromArgb(200, 0xFF, 0xFF, 0xFF), 0.5));
            brush.GradientStops.Add(new GradientStop(Color.FromArgb(0, 0xFF, 0xFF, 0xFF), 1.0));
            brush.Freeze();
            return brush;
        }

        private static FontFamily FindFontFamily(string name)
        {
            foreach (FontFamily family in Fonts.SystemFontFamilies)
            {
                if (string.Equals(family.Source, name, StringComparison.OrdinalIgnoreCase))
                {
                    return family;
                }
            }
            return null;
        }

        private static FontFamily PickFontFamily(string preferred)
        {
            return FindFontFamily(preferred) ?? new FontFamily("Segoe UI");
        }

        private static FontFamily englishFont;
        private static bool englishFontTried;

        private static FontFamily PickChineseFont()
        {
            FontFamily family = FindFontFamily("Microsoft YaHei UI");
            if (family == null) family = FindFontFamily("Microsoft YaHei");
            if (family == null) family = FindFontFamily("SimHei");
            if (family == null) family = FindFontFamily("SimSun");
            return family ?? PickFontFamily("Segoe UI");
        }

        private static FontFamily PickEnglishFont()
        {
            FontFamily maple = LoadMapleFont();
            return maple ?? PickFontFamily("Segoe UI Variable Text");
        }

        private static FontFamily LoadMapleFont()
        {
            if (englishFontTried) return englishFont;
            englishFontTried = true;
            try
            {
                string fontDirectory = IOPath.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    "OffWorkCountdown",
                    "Fonts",
                    "Maple");
                System.IO.Directory.CreateDirectory(fontDirectory);
                WriteEmbeddedResource(
                    "OffWorkCountdown.Fonts.MapleMono-Regular.ttf",
                    IOPath.Combine(fontDirectory, "MapleMono-Regular.ttf"));
                WriteEmbeddedResource(
                    "OffWorkCountdown.Fonts.MapleMono-Bold.ttf",
                    IOPath.Combine(fontDirectory, "MapleMono-Bold.ttf"));

                System.Collections.Generic.ICollection<FontFamily> families =
                    Fonts.GetFontFamilies(fontDirectory);
                foreach (FontFamily family in families)
                {
                    englishFont = family;
                    break;
                }
            }
            catch
            {
                englishFont = null;
            }
            return englishFont;
        }

        private static void WriteEmbeddedResource(string resourceName, string targetFile)
        {
            System.Reflection.Assembly assembly =
                System.Reflection.Assembly.GetExecutingAssembly();
            using (System.IO.Stream stream = assembly.GetManifestResourceStream(resourceName))
            {
                if (stream == null) return;
                byte[] bytes = new byte[stream.Length];
                int offset = 0;
                while (offset < bytes.Length)
                {
                    int read = stream.Read(bytes, offset, bytes.Length - offset);
                    if (read <= 0) break;
                    offset += read;
                }
                if (!System.IO.File.Exists(targetFile) ||
                    new System.IO.FileInfo(targetFile).Length != bytes.Length)
                {
                    System.IO.File.WriteAllBytes(targetFile, bytes);
                }
            }
        }

        private static IntPtr CreateTrayIconHandle()
        {
            try
            {
                using (Drawing.Bitmap bitmap = new Drawing.Bitmap(
                    32,
                    32,
                    Drawing.Imaging.PixelFormat.Format32bppArgb))
                {
                    using (Drawing.Graphics graphics = Drawing.Graphics.FromImage(bitmap))
                    {
                        graphics.SmoothingMode = Drawing2D.SmoothingMode.AntiAlias;
                        graphics.Clear(Drawing.Color.Transparent);
                        using (Drawing.SolidBrush circleBrush = new Drawing.SolidBrush(
                            Drawing.Color.FromArgb(255, 15, 108, 189)))
                        {
                            graphics.FillEllipse(circleBrush, 1, 1, 30, 30);
                        }
                        using (Drawing.Pen pen = new Drawing.Pen(
                            Drawing.Color.White,
                            2.4f))
                        {
                            pen.StartCap = Drawing2D.LineCap.Round;
                            pen.EndCap = Drawing2D.LineCap.Round;
                            graphics.DrawEllipse(pen, 8, 8, 16, 16);
                            graphics.DrawLine(pen, 16, 11, 16, 17);
                            graphics.DrawLine(pen, 16, 17, 20, 19.5f);
                        }
                    }
                    return bitmap.GetHicon();
                }
            }
            catch
            {
                return IntPtr.Zero;
            }
        }

        protected override void OnClosed(EventArgs e)
        {
            Microsoft.Win32.SystemEvents.DisplaySettingsChanged -= OnDisplaySettingsChanged;
            timer.Stop();
            if (dockSlideTimer != null) dockSlideTimer.Stop();
            if (dockHideTimer != null) dockHideTimer.Stop();
            if (chimePlayer != null)
            {
                try
                {
                    chimePlayer.Stop();
                    chimePlayer.Dispose();
                }
                catch
                {
                }
                chimePlayer = null;
            }
            if (chimeStream != null)
            {
                chimeStream.Dispose();
                chimeStream = null;
            }
            if (tray != null)
            {
                tray.Visible = false;
                tray.Dispose();
                tray = null;
            }
            if (trayDrawingIcon != null)
            {
                trayDrawingIcon.Dispose();
                trayDrawingIcon = null;
            }
            if (trayIconHandle != IntPtr.Zero)
            {
                NativeMethods.DestroyIcon(trayIconHandle);
                trayIconHandle = IntPtr.Zero;
            }
            base.OnClosed(e);
        }

        #endregion
    }

    internal static class NativeMethods
    {
        public const int GWL_EXSTYLE = -20;
        public const int WS_EX_TOOLWINDOW = 0x00000080;

        public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

        /// <summary>第二个实例通过系统消息唤醒已隐藏的挂件；消息号由字符串注册，不会冲突</summary>
        public static readonly int ShowWidgetMessage = RegisterWindowMessage(
            "OffWorkCountdown.ShowWidget.v1");

        [DllImport("user32.dll", EntryPoint = "GetWindowLong")]
        public static extern int GetWindowLong(IntPtr hWnd, int nIndex);

        [DllImport("user32.dll", EntryPoint = "SetWindowLong")]
        public static extern int SetWindowLong(IntPtr hWnd, int nIndex, int dwNewLong);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern int RegisterWindowMessage(string message);

        [DllImport("user32.dll")]
        public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);

        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);

        [DllImport("user32.dll")]
        public static extern bool PostMessage(IntPtr hWnd, int message, IntPtr wParam, IntPtr lParam);

        [DllImport("user32.dll", SetLastError = true)]
        public static extern bool DestroyIcon(IntPtr hIcon);

        public static IntPtr FindWindowByTitle(string title)
        {
            IntPtr result = IntPtr.Zero;
            EnumWindows(
                delegate(IntPtr window, IntPtr parameter)
                {
                    StringBuilder text = new StringBuilder(256);
                    GetWindowText(window, text, text.Capacity);
                    if (text.ToString() == title)
                    {
                        result = window;
                        return false;
                    }
                    return true;
                },
                IntPtr.Zero);
            return result;
        }
    }

    internal static class Program
    {
        private static Mutex appMutex;

        [STAThread]
        public static void Main()
        {
            bool createdNew;
            appMutex = new Mutex(true, "Local\\OffWorkCountdown.Widget", out createdNew);
            if (!createdNew)
            {
                IntPtr existing = NativeMethods.FindWindowByTitle("下班倒计时");
                if (existing != IntPtr.Zero && NativeMethods.ShowWidgetMessage != 0)
                {
                    NativeMethods.PostMessage(
                        existing,
                        NativeMethods.ShowWidgetMessage,
                        IntPtr.Zero,
                        IntPtr.Zero);
                }
                return;
            }

            Application app = new Application();
            app.ShutdownMode = ShutdownMode.OnExplicitShutdown;
            try
            {
                app.Run(new CountdownWindow());
            }
            finally
            {
                appMutex.ReleaseMutex();
                appMutex.Dispose();
            }
        }
    }
    /// <summary>
    /// 极光像素着色器：双色光带 + 垂直光帘，时间常量由窗口每帧推进。
    /// </summary>
    internal sealed class AuroraEffect : ShaderEffect
    {
        private static readonly PixelShader Shader = LoadShader();

        public static readonly DependencyProperty InputProperty =
            ShaderEffect.RegisterPixelShaderSamplerProperty("Input", typeof(AuroraEffect), 0);

        public static readonly DependencyProperty TimeProperty =
            DependencyProperty.Register(
                "Time",
                typeof(double),
                typeof(AuroraEffect),
                new UIPropertyMetadata(0.0, PixelShaderConstantCallback(0)));

        public static readonly DependencyProperty ResolutionProperty =
            DependencyProperty.Register(
                "Resolution",
                typeof(Point),
                typeof(AuroraEffect),
                new UIPropertyMetadata(new Point(300.0, 146.0), PixelShaderConstantCallback(1)));

        public static bool IsAvailable
        {
            get { return Shader != null; }
        }

        public AuroraEffect()
        {
            PixelShader = Shader;
            UpdateShaderValue(InputProperty);
            UpdateShaderValue(TimeProperty);
            UpdateShaderValue(ResolutionProperty);
        }

        public Brush Input
        {
            get { return (Brush)GetValue(InputProperty); }
            set { SetValue(InputProperty, value); }
        }

        public double Time
        {
            get { return (double)GetValue(TimeProperty); }
            set { SetValue(TimeProperty, value); }
        }

        public Point Resolution
        {
            get { return (Point)GetValue(ResolutionProperty); }
            set { SetValue(ResolutionProperty, value); }
        }

        private static PixelShader LoadShader()
        {
            Stream stream = typeof(AuroraEffect).Assembly.GetManifestResourceStream("Aurora.ps");
            if (stream == null) return null;
            using (stream)
            {
                PixelShader shader = new PixelShader();
                shader.SetStreamSource(stream);
                return shader;
            }
        }
    }
    internal sealed class OceanEffect : ShaderEffect
    {
        private static readonly PixelShader Shader = LoadShader();

        public static readonly DependencyProperty InputProperty =
            ShaderEffect.RegisterPixelShaderSamplerProperty("Input", typeof(OceanEffect), 0);

        public static readonly DependencyProperty TimeProperty =
            DependencyProperty.Register(
                "Time",
                typeof(double),
                typeof(OceanEffect),
                new UIPropertyMetadata(0.0, PixelShaderConstantCallback(0)));

        public static readonly DependencyProperty ResolutionProperty =
            DependencyProperty.Register(
                "Resolution",
                typeof(Point),
                typeof(OceanEffect),
                new UIPropertyMetadata(new Point(300.0, 146.0), PixelShaderConstantCallback(1)));

        public static bool IsAvailable
        {
            get { return Shader != null; }
        }

        public OceanEffect()
        {
            PixelShader = Shader;
            UpdateShaderValue(InputProperty);
            UpdateShaderValue(TimeProperty);
            UpdateShaderValue(ResolutionProperty);
        }

        public Brush Input
        {
            get { return (Brush)GetValue(InputProperty); }
            set { SetValue(InputProperty, value); }
        }

        public double Time
        {
            get { return (double)GetValue(TimeProperty); }
            set { SetValue(TimeProperty, value); }
        }

        public Point Resolution
        {
            get { return (Point)GetValue(ResolutionProperty); }
            set { SetValue(ResolutionProperty, value); }
        }

        private static PixelShader LoadShader()
        {
            Stream stream = typeof(OceanEffect).Assembly.GetManifestResourceStream("Ocean.ps");
            if (stream == null) return null;
            using (stream)
            {
                PixelShader shader = new PixelShader();
                shader.SetStreamSource(stream);
                return shader;
            }
        }
    }
}

