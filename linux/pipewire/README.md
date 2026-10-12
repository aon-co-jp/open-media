# Linux: 全アプリの音をアップサンプリング(PipeWire) / System-wide upsampling on Linux (PipeWire)

**日本語**: 置き場所に2つの設定ファイルを置いてPipeWireを再起動すると、ブラウザ・YouTube・音楽アプリなど全ての音が、
最高品質の変換(resample.quality=14)で高いレートへ変換されて出力されます。DACの対応レートに合わせて
`default.clock.rate` を編集してください。**この設定は実機のPipeWire環境では未検証です**(開発環境のWSLには音声デバイスがありません)。

```
mkdir -p ~/.config/pipewire/pipewire.conf.d ~/.config/pipewire/pipewire-pulse.conf.d
cp 10-open-media-upsample.conf ~/.config/pipewire/pipewire.conf.d/
cp 10-open-media-pulse.conf    ~/.config/pipewire/pipewire-pulse.conf.d/
systemctl --user restart pipewire pipewire-pulse wireplumber
pw-metadata -n settings 0 clock.force-rate 0   # 設定確認: pw-top でレートを見る
```

**English**: Put the two config files in place and restart PipeWire; every app's audio (browsers, YouTube, music apps) is then converted to a higher
rate with the highest-quality resampler (resample.quality=14). Edit `default.clock.rate` to match your DAC. **Untested on a real PipeWire
setup** (the WSL development environment has no audio device).

DRM-protected streams may bypass processing in some players; nothing here removes protection.
