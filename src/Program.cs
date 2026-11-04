// 下班倒计时桌面挂件
// 目标框架：.NET Framework 4.8（Windows 10/11 自带，无需另装运行时）
using System;
using System.ComponentModel;
using System.Globalization;
using System.IO;
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
[assembly: System.Reflection.AssemblyVersion("1.0.0.0")]
[assembly: System.Reflection.AssemblyFileVersion("1.0.0.0")]

namespace OffWorkCountdown
{
    [DataContract]
    internal sealed class CountdownSettings
    {
        public const string DefaultStart = "09:00";
        public const string DefaultEnd = "18:30";

        [DataMember(Name = "startTime")]
        public string StartTime { get; set; }

        [DataMember(Name = "endTime")]
        public string EndTime { get; set; }

        [DataMember(Name = "windowRight", EmitDefaultValue = false)]
        public double? WindowRight { get; set; }

        [DataMember(Name = "windowTop", EmitDefaultValue = false)]
        public double? WindowTop { get; set; }

        public CountdownSettings()
        {
            StartTime = DefaultStart;
            EndTime = DefaultEnd;
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
        private const string SettingsHint = "格式 HH:mm，下班时间需晚于上班时间";
        private const double ExpandedWidth = 280.0;
        private const double CollapsedWidth = 184.0;
        private const double WindowMargin = 18.0;
        private const double DockThreshold = 10.0;
        private const double DockSliver = 6.0;
        private const double DockSlideMs = 170.0;
        private const double DockHideDelayMs = 700.0;

        private static readonly SolidColorBrush TextPrimary = MakeBrush(0x1A, 0x23, 0x32);
        private static readonly SolidColorBrush TextSecondary = MakeBrush(0x5C, 0x67, 0x73);
        private static readonly SolidColorBrush TextDisabled = MakeBrush(0xA3, 0xAB, 0xB4);
        private static readonly SolidColorBrush BorderLine = MakeBrush(0xE3, 0xE8, 0xEF);
        private static readonly SolidColorBrush Surface = MakeBrush(0xFF, 0xFF, 0xFF);
        private static readonly SolidColorBrush SurfaceMuted = MakeBrush(0xF7, 0xF9, 0xFB);
        private static readonly SolidColorBrush Track = MakeBrush(0xEE, 0xF1, 0xF4);
        private static readonly SolidColorBrush Brand = MakeBrush(0x05, 0x70, 0xEE);
        private static readonly SolidColorBrush BrandHover = MakeBrush(0x04, 0x54, 0xB2);
        private static readonly SolidColorBrush Success = MakeBrush(0x16, 0xA3, 0x4A);
        private static readonly SolidColorBrush Danger = MakeBrush(0xDC, 0x26, 0x26);
        private static readonly SolidColorBrush HoverFill = MakeBrush(0xF3, 0xF7, 0xFB);
        private static readonly SolidColorBrush PressedFill = MakeBrush(0xE7, 0xEF, 0xF9);

        private readonly CountdownSettings settings;
        private readonly DispatcherTimer timer;

        private Grid surface;
        private Border shadow;
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
        private TextBlock footerRightText;
        private TextBlock collapsedLabel;
        private TextBlock collapsedTime;
        private TextBlock settingsError;
        private Grid progressTrack;
        private Border progressFill;
        private TextBox startBox;
        private TextBox endBox;

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
        private bool focusSettingsAfterTransition;
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

        public CountdownWindow()
        {
            settings = SettingsStore.Load();
            RefreshTimeSpans();

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
            FontFamily = new FontFamily("Segoe UI");
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
            Height = maxCardHeight + WindowMargin * 2;

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
        }

        private void OnSizeChanged(object sender, SizeChangedEventArgs e)
        {
            if (!IsLoaded) return;
            if (anchorReady) ApplyAnchor();
            UpdateProgressWidth();
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

        private Grid BuildRoot()
        {
            Grid root = new Grid();
            root.Margin = new Thickness(WindowMargin);

            surface = new Grid();
            surface.HorizontalAlignment = HorizontalAlignment.Right;
            surface.VerticalAlignment = VerticalAlignment.Top;

            shadow = new Border();
            shadow.Background = Surface;
            shadow.CornerRadius = new CornerRadius(14);
            shadow.IsHitTestVisible = false;
            DropShadowEffect effect = new DropShadowEffect();
            effect.Color = Color.FromRgb(15, 23, 42);
            effect.BlurRadius = 26;
            effect.ShadowDepth = 4;
            effect.Direction = 270;
            effect.Opacity = 0.16;
            effect.RenderingBias = RenderingBias.Performance;
            shadow.Effect = effect;

            card = new Border();
            card.Background = Surface;
            card.BorderBrush = BorderLine;
            card.BorderThickness = new Thickness(1);
            card.CornerRadius = new CornerRadius(14);
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

            Grid contentHost = new Grid();
            contentHost.ClipToBounds = true;
            contentHost.Children.Add(countdownView);
            contentHost.Children.Add(settingsView);
            contentHost.Children.Add(collapsedView);
            card.Child = contentHost;
            card.ContextMenu = BuildContextMenu();
            card.ContextMenu.Closed += delegate { ScheduleDockHide(); };
            card.MouseEnter += OnCardMouseEnter;
            card.MouseLeave += OnCardMouseLeave;

            surface.Children.Add(shadow);
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
            view.Margin = new Thickness(18, 14, 18, 14);

            Grid header = new Grid();

            StackPanel status = new StackPanel();
            status.Orientation = Orientation.Horizontal;
            status.VerticalAlignment = VerticalAlignment.Center;

            headerIcon = new ContentControl();
            headerIcon.Width = 16;
            headerIcon.Height = 16;
            headerIcon.Margin = new Thickness(0, 0, 6, 0);
            headerIcon.VerticalAlignment = VerticalAlignment.Center;
            headerIcon.Content = CreateClockIcon(16, TextSecondary);

            statusLabel = MakeText("距离下班", 12, FontWeights.Normal, TextSecondary);
            status.Children.Add(headerIcon);
            status.Children.Add(statusLabel);

            StackPanel actions = new StackPanel();
            actions.Orientation = Orientation.Horizontal;
            actions.HorizontalAlignment = HorizontalAlignment.Right;
            actions.VerticalAlignment = VerticalAlignment.Top;

            Button settingsButton = CreateIconButton(
                CreateSlidersIcon(16, TextSecondary),
                "设置",
                delegate { ShowSettings(); });
            Button collapseButton = CreateIconButton(
                CreateMinusIcon(16, TextSecondary),
                "收起",
                delegate { Collapse(); });
            settingsButton.Margin = new Thickness(0, 0, 2, 0);
            actions.Children.Add(settingsButton);
            actions.Children.Add(collapseButton);

            header.Children.Add(status);
            header.Children.Add(actions);
            view.Children.Add(header);

            Grid timeHost = new Grid();
            timeHost.Height = 38;
            timeHost.Margin = new Thickness(0, 6, 0, 0);

            countdownText = MakeText("00:00:00", 32, FontWeights.SemiBold, TextPrimary);
            countdownText.VerticalAlignment = VerticalAlignment.Center;
            countdownText.LineHeight = 37;
            Typography.SetNumeralAlignment(countdownText, FontNumeralAlignment.Tabular);

            doneText = MakeText("今天辛苦了。", 24, FontWeights.SemiBold, Success);
            doneText.VerticalAlignment = VerticalAlignment.Center;
            doneText.Visibility = Visibility.Collapsed;

            timeHost.Children.Add(countdownText);
            timeHost.Children.Add(doneText);
            view.Children.Add(timeHost);

            progressTrack = new Grid();
            progressTrack.Height = 3;
            progressTrack.Margin = new Thickness(0, 12, 0, 0);
            progressTrack.ClipToBounds = true;
            progressTrack.SizeChanged += delegate { UpdateProgressWidth(); };

            Border trackBackground = new Border();
            trackBackground.Background = Track;
            trackBackground.CornerRadius = new CornerRadius(1.5);

            progressFill = new Border();
            progressFill.Background = TextPrimary;
            progressFill.CornerRadius = new CornerRadius(1.5);
            progressFill.HorizontalAlignment = HorizontalAlignment.Left;

            progressTrack.Children.Add(trackBackground);
            progressTrack.Children.Add(progressFill);
            view.Children.Add(progressTrack);

            Grid footer = new Grid();
            footer.Margin = new Thickness(0, 8, 0, 0);

            footerLeftText = MakeText("下班时间 18:30", 12, FontWeights.Normal, TextSecondary);
            footerRightText = MakeText("100%", 12, FontWeights.Normal, TextSecondary);
            footerRightText.HorizontalAlignment = HorizontalAlignment.Right;
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
            TextBlock title = MakeText("倒计时设置", 14, FontWeights.SemiBold, TextPrimary);
            title.VerticalAlignment = VerticalAlignment.Center;
            Button closeButton = CreateIconButton(
                CreateCloseIcon(16, TextSecondary),
                "关闭设置",
                delegate { ShowCountdown(); });
            closeButton.HorizontalAlignment = HorizontalAlignment.Right;
            header.Children.Add(title);
            header.Children.Add(closeButton);
            view.Children.Add(header);

            settingsError = MakeText(SettingsHint, 10.5, FontWeights.Normal, TextDisabled);
            settingsError.Margin = new Thickness(0, 6, 0, 0);
            settingsError.MinHeight = 16;
            settingsError.TextWrapping = TextWrapping.Wrap;

            Grid fields = new Grid();
            fields.Margin = new Thickness(0, 12, 0, 0);
            fields.ColumnDefinitions.Add(new ColumnDefinition());
            fields.ColumnDefinitions.Add(new ColumnDefinition());

            Border startBorder = CreateTimeInput(out startBox);
            Border endBorder = CreateTimeInput(out endBox);
            startBox.Text = settings.StartTime;
            endBox.Text = settings.EndTime;

            StackPanel startField = new StackPanel();
            startField.Children.Add(MakeText("上班时间", 11, FontWeights.Normal, TextSecondary));
            startBorder.Margin = new Thickness(0, 5, 0, 0);
            startField.Children.Add(startBorder);

            StackPanel endField = new StackPanel();
            endField.Margin = new Thickness(10, 0, 0, 0);
            endField.Children.Add(MakeText("下班时间", 11, FontWeights.Normal, TextSecondary));
            endBorder.Margin = new Thickness(0, 5, 0, 0);
            endField.Children.Add(endBorder);

            Grid.SetColumn(startField, 0);
            Grid.SetColumn(endField, 1);
            fields.Children.Add(startField);
            fields.Children.Add(endField);
            view.Children.Add(fields);

            view.Children.Add(settingsError);

            Grid footer = new Grid();
            footer.Margin = new Thickness(0, 11, 0, 0);

            Button restore = CreateLinkButton("恢复默认", delegate { ResetSettings(); });
            restore.HorizontalAlignment = HorizontalAlignment.Left;

            StackPanel actions = new StackPanel();
            actions.Orientation = Orientation.Horizontal;
            actions.HorizontalAlignment = HorizontalAlignment.Right;

            Button cancel = CreateActionButton("取消", false, delegate { ShowCountdown(); });
            cancel.Margin = new Thickness(0, 0, 8, 0);
            Button save = CreateActionButton("保存", true, delegate { SaveSettings(); });
            actions.Children.Add(cancel);
            actions.Children.Add(save);

            footer.Children.Add(restore);
            footer.Children.Add(actions);
            view.Children.Add(footer);

            startBox.KeyDown += OnSettingsKeyDown;
            endBox.KeyDown += OnSettingsKeyDown;
            AutomationProperties.SetName(startBox, "上班时间");
            AutomationProperties.SetName(endBox, "下班时间");

            return view;
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

            collapsedLabel = MakeText("距离下班", 11, FontWeights.Normal, TextSecondary);
            collapsedLabel.VerticalAlignment = VerticalAlignment.Center;
            collapsedLabel.Margin = new Thickness(0, 0, 6, 0);

            collapsedTime = MakeText("00:00:00", 13, FontWeights.SemiBold, TextPrimary);
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
            ToolTipService.SetToolTip(view, "拖动调整位置，点击展开");
            AutomationProperties.SetName(view, "展开下班倒计时");
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
            settingsItem.Header = "设置";
            settingsItem.Click += delegate { ShowSettings(); };

            MenuItem hideItem = new MenuItem();
            hideItem.Header = "隐藏窗口";
            hideItem.Click += delegate
            {
                CancelDockHide();
                StopDockSlide();
                Hide();
                UpdateTrayToggleText();
            };

            MenuItem exitItem = new MenuItem();
            exitItem.Header = "退出";
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
            border.SetValue(Border.CornerRadiusProperty, new CornerRadius(5));
            FrameworkElementFactory presenter = new FrameworkElementFactory(typeof(ContentPresenter));
            presenter.SetValue(FrameworkElement.HorizontalAlignmentProperty, HorizontalAlignment.Center);
            presenter.SetValue(FrameworkElement.VerticalAlignmentProperty, VerticalAlignment.Center);
            border.AppendChild(presenter);
            template.VisualTree = border;

            Trigger hover = new Trigger();
            hover.Property = UIElement.IsMouseOverProperty;
            hover.Value = true;
            hover.Setters.Add(new Setter(Border.BackgroundProperty, HoverFill, "bg"));
            template.Triggers.Add(hover);

            Trigger pressed = new Trigger();
            pressed.Property = ButtonBase.IsPressedProperty;
            pressed.Value = true;
            pressed.Setters.Add(new Setter(Border.BackgroundProperty, PressedFill, "bg"));
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
            SolidColorBrush pressedBrush = primary ? BrandHover : PressedFill;

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
            border.SetValue(Border.CornerRadiusProperty, new CornerRadius(7));
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

        private Border CreateTimeInput(out TextBox input)
        {
            Border shell = new Border();
            shell.Height = 32;
            shell.Padding = new Thickness(8, 0, 8, 0);
            shell.Background = SurfaceMuted;
            shell.BorderBrush = BorderLine;
            shell.BorderThickness = new Thickness(1);
            shell.CornerRadius = new CornerRadius(7);

            input = new TextBox();
            input.MaxLength = 5;
            input.FontSize = 13;
            input.Foreground = TextPrimary;
            input.CaretBrush = TextPrimary;
            input.Background = Brushes.Transparent;
            input.BorderThickness = new Thickness(0);
            input.Padding = new Thickness(0);
            input.VerticalContentAlignment = VerticalAlignment.Center;
            input.FocusVisualStyle = null;
            shell.Child = input;

            input.GotFocus += delegate
            {
                shell.Background = Surface;
                shell.BorderBrush = Brand;
            };
            input.LostFocus += delegate
            {
                shell.Background = SurfaceMuted;
                shell.BorderBrush = BorderLine;
            };
            input.TextChanged += delegate { ResetSettingsMessage(); };
            return shell;
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
            tray.Text = AppTitle;
            tray.Visible = true;
            tray.DoubleClick += delegate { ShowWidget(); };

            WinForms.ContextMenuStrip menu = new WinForms.ContextMenuStrip();
            trayToggleItem = new WinForms.ToolStripMenuItem();
            trayToggleItem.Text = "隐藏窗口";
            trayToggleItem.Click += delegate { ToggleVisibility(); };

            WinForms.ToolStripMenuItem settingsItem = new WinForms.ToolStripMenuItem();
            settingsItem.Text = "设置";
            settingsItem.Click += delegate
            {
                ShowWidget();
                Activate();
                ShowSettings();
            };

            WinForms.ToolStripMenuItem exitItem = new WinForms.ToolStripMenuItem();
            exitItem.Text = "退出";
            exitItem.Click += delegate { ExitApp(); };

            menu.Items.Add(trayToggleItem);
            menu.Items.Add(settingsItem);
            menu.Items.Add(new WinForms.ToolStripSeparator());
            menu.Items.Add(exitItem);
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
            trayToggleItem.Text = IsVisible ? "隐藏窗口" : "显示窗口";
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

            double cardWidth = surface.Width;
            double cardHeight = surface.Height;
            if (double.IsNaN(cardWidth) || cardWidth <= 0)
            {
                cardWidth = Math.Max(0.0, Width - WindowMargin * 2);
            }
            if (double.IsNaN(cardHeight) || cardHeight <= 0)
            {
                cardHeight = Math.Max(0.0, Height - WindowMargin * 2);
            }
            if (cardWidth <= 0 || cardHeight <= 0) return;

            // 只保证卡片本身在屏幕内；窗口四周/下方多出来的透明区域可以挂到屏幕外，
            // 这样拖到屏幕底部时不会被误判为“挂出屏幕”而弹回顶部。
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            double newRight = Clamp(
                anchorRight,
                work.Left + cardWidth + WindowMargin,
                work.Right + WindowMargin);
            double newTop = Clamp(
                anchorTop,
                work.Top - WindowMargin,
                work.Bottom - cardHeight - WindowMargin);

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

        #region 贴边隐藏

        private static double Clamp(double value, double min, double max)
        {
            if (value < min) return min;
            if (value > max) return max;
            return value;
        }

        private DockEdge FindDockEdge(Rect work)
        {
            double cardWidth = surface.Width;
            double cardHeight = surface.Height;
            double cardRight = Left + Width - WindowMargin;
            double cardLeft = cardRight - cardWidth;
            double cardTop = Top + WindowMargin;
            double cardBottom = cardTop + cardHeight;

            double bestDistance = DockThreshold;
            DockEdge best = DockEdge.None;
            double distance = cardLeft - work.Left;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Left;
            }
            distance = work.Right - cardRight;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Right;
            }
            distance = cardTop - work.Top;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Top;
            }
            distance = work.Bottom - cardBottom;
            if (distance < bestDistance)
            {
                bestDistance = distance;
                best = DockEdge.Bottom;
            }
            return best;
        }

        private void DockAt(DockEdge edge, Rect work)
        {
            dockEdge = edge;
            double cardWidth = surface.Width;
            double cardHeight = surface.Height;

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
                    : work.Bottom - cardHeight - WindowMargin;
            }

            hasCustomPosition = true;
            PersistPosition();
            HideToDock();
        }

        private void RepositionDockedView()
        {
            if (dockEdge == DockEdge.None) return;
            double cardWidth = surface.Width;
            double cardHeight = surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);

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
                    : work.Bottom - cardHeight - WindowMargin;
            }

            ApplyAnchor();
            PersistPosition();
        }

        private void GetDockPosition(bool hidden, out double left, out double top)
        {
            double cardWidth = surface.Width;
            double cardHeight = surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
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
                        ? work.Bottom - DockSliver - WindowMargin
                        : work.Bottom - cardHeight - WindowMargin;
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
            CancelDockHide();
            double left;
            double top;
            GetDockPosition(false, out left, out top);
            StartDockSlide(left, top);
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
            // 一旦开始拖动就先离开贴边状态，松手后再重新判断落在哪一侧。
            dockEdge = DockEdge.None;
            dockedHidden = false;
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

            double cardWidth = surface.Width;
            double cardHeight = surface.Height;
            Rect work = WorkAreaForCard(anchorRight, anchorTop, cardWidth, cardHeight);
            DockEdge edge = FindDockEdge(work);
            if (edge != DockEdge.None)
            {
                DockAt(edge, work);
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
            focusSettingsAfterTransition = false;
            RefreshUi();
            SwitchView(countdownView, ExpandedWidth, countdownCardHeight);
        }

        private void ShowSettings()
        {
            settingsOpen = true;
            startBox.Text = settings.StartTime;
            endBox.Text = settings.EndTime;
            ResetSettingsMessage();
            if (currentView == settingsView)
            {
                FocusSettingsStart();
                return;
            }
            focusSettingsAfterTransition = true;
            SwitchView(settingsView, ExpandedWidth, settingsCardHeight);
        }

        private void Collapse()
        {
            settingsOpen = false;
            focusSettingsAfterTransition = false;
            RefreshUi();
            SwitchView(collapsedView, CollapsedWidth, collapsedCardHeight);
        }

        private void FocusSettingsStart()
        {
            Dispatcher.BeginInvoke(
                DispatcherPriority.Input,
                new Action(delegate
                {
                    if (settingsView.Visibility != Visibility.Visible) return;
                    startBox.Focus();
                    startBox.SelectAll();
                }));
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

        private static void SetViewState(FrameworkElement view, FrameworkElement visibleView)
        {
            bool visible = view == visibleView;
            view.Visibility = visible ? Visibility.Visible : Visibility.Collapsed;
            view.Opacity = visible ? 1 : 0;
            view.IsHitTestVisible = visible;
        }

        private void Reveal(FrameworkElement target)
        {
            SetViewState(countdownView, target);
            SetViewState(settingsView, target);
            SetViewState(collapsedView, target);
            currentView = target;
        }

        /// <summary>
        /// 从当前视觉状态开始过渡；如果在动画途中再次触发，会从当前尺寸和透明度接着走，
        /// 不会把点击吞掉，也不会跳回起点。
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

            // 不做尺寸动画：直接切换内容和卡片大小。顶层窗口固定为最大尺寸，
            // 所以不会出现窗口和内容差一帧导致的文字裁切或跳动。
            surface.Width = targetWidth;
            surface.Height = targetHeight;
            Reveal(target);

            // 尺寸变了以后贴边的锚点要重算；没贴边则重新保证卡片在屏幕内。
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

            if (focusSettingsAfterTransition && target == settingsView)
            {
                focusSettingsAfterTransition = false;
                FocusSettingsStart();
            }
        }

        private void OnSettingsKeyDown(object sender, KeyEventArgs e)
        {
            if (e.Key == Key.Enter)
            {
                SaveSettings();
                e.Handled = true;
            }
            else if (e.Key == Key.Escape)
            {
                ShowCountdown();
                e.Handled = true;
            }
        }

        private void ResetSettings()
        {
            startBox.Text = CountdownSettings.DefaultStart;
            endBox.Text = CountdownSettings.DefaultEnd;
            ResetSettingsMessage();
            startBox.Focus();
            startBox.SelectAll();
        }

        private void SaveSettings()
        {
            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            if (!CountdownSettings.TryNormalizeTime(startBox.Text, out startText, out start))
            {
                ShowSettingsError("上班时间格式应为 HH:mm");
                startBox.Focus();
                startBox.SelectAll();
                return;
            }
            if (!CountdownSettings.TryNormalizeTime(endBox.Text, out endText, out end))
            {
                ShowSettingsError("下班时间格式应为 HH:mm");
                endBox.Focus();
                endBox.SelectAll();
                return;
            }
            if (end <= start)
            {
                ShowSettingsError("下班时间需要晚于上班时间");
                endBox.Focus();
                endBox.SelectAll();
                return;
            }

            settings.StartTime = startText;
            settings.EndTime = endText;
            workStart = start;
            workEnd = end;
            ResetSettingsMessage();
            SettingsStore.Save(settings);
            ShowCountdown();
        }

        private void ShowSettingsError(string message)
        {
            settingsError.Text = message;
            settingsError.Foreground = Danger;
        }

        private void ResetSettingsMessage()
        {
            if (settingsError == null) return;
            settingsError.Text = SettingsHint;
            settingsError.Foreground = TextDisabled;
        }

        private void RefreshTimeSpans()
        {
            string startText;
            string endText;
            TimeSpan start;
            TimeSpan end;
            if (!CountdownSettings.TryNormalizeTime(settings.StartTime, out startText, out start) ||
                !CountdownSettings.TryNormalizeTime(settings.EndTime, out endText, out end) ||
                end <= start)
            {
                settings.StartTime = CountdownSettings.DefaultStart;
                settings.EndTime = CountdownSettings.DefaultEnd;
                start = TimeSpan.FromHours(9);
                end = TimeSpan.FromHours(18.5);
            }
            workStart = start;
            workEnd = end;
        }

        #endregion

        #region 刷新与绘制

        private void RefreshUi()
        {
            DateTime now = DateTime.Now;
            DateTime endAt = now.Date.Add(workEnd);
            DateTime startAt = now.Date.Add(workStart);
            bool working = now < endAt;

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

            if (lastWorking == null || lastWorking.Value != working)
            {
                headerIcon.Content = working
                    ? (object)CreateClockIcon(16, TextSecondary)
                    : (object)CreateCheckIcon(16, Success);
                collapsedIcon.Content = working
                    ? (object)CreateClockIcon(16, TextSecondary)
                    : (object)CreateCheckIcon(16, Success);
                lastWorking = working;
            }

            statusLabel.Text = working ? "距离下班" : "已下班";
            statusLabel.Foreground = working ? TextSecondary : Success;
            countdownText.Visibility = working ? Visibility.Visible : Visibility.Collapsed;
            doneText.Visibility = working ? Visibility.Collapsed : Visibility.Visible;
            if (countdownText.Text != countdown) countdownText.Text = countdown;
            footerLeftText.Text = working
                ? "下班时间 " + settings.EndTime
                : "明天 " + settings.EndTime;
            footerRightText.Text = percent + "%";
            progressFill.Background = working ? TextPrimary : Success;

            collapsedLabel.Text = working ? "距离下班" : "已下班";
            collapsedLabel.Foreground = working ? TextSecondary : Success;
            collapsedTime.Text = working ? countdown : "明天见";
            collapsedTime.Foreground = working ? TextPrimary : Success;

            UpdateProgressWidth();

            if (tray != null)
            {
                string tip = working
                    ? "距下班 " + countdown
                    : "已下班，明天 " + settings.EndTime;
                if (tray.Text != tip) tray.Text = tip;
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
                            Drawing.Color.FromArgb(255, 5, 112, 238)))
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
}

