# The volume of the default output device for the E2E test (see volume.rs), from 0 to 1. It uses Core Audio through
# C#, not the code of Moulin. volume.rs sets $Action and $Value before this script:
# "get" writes the volume, "set" sets $Value, and "watch" writes the time (ms since 1970) and the volume at each change.
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

[Guid("5CDF2C82-841E-4546-9722-0CF74078229A"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IAudioEndpointVolume {
  int RegisterControlChangeNotify(IntPtr callback);
  int UnregisterControlChangeNotify(IntPtr callback);
  int GetChannelCount(out uint count);
  int SetMasterVolumeLevel(float decibels, ref Guid context);
  int SetMasterVolumeLevelScalar(float level, ref Guid context);
  int GetMasterVolumeLevel(out float decibels);
  int GetMasterVolumeLevelScalar(out float level);
}

[Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDevice {
  int Activate(ref Guid iid, int context, IntPtr parameters, [MarshalAs(UnmanagedType.IUnknown)] out object endpoint);
}

[Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IMMDeviceEnumerator {
  int EnumAudioEndpoints(int flow, int mask, out IntPtr devices);
  int GetDefaultAudioEndpoint(int flow, int role, out IMMDevice device);
}

[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
class MMDeviceEnumerator { }

public static class MoulinTestVolume {
  static IAudioEndpointVolume Endpoint() {
    var devices = (IMMDeviceEnumerator)new MMDeviceEnumerator();
    IMMDevice device;
    Marshal.ThrowExceptionForHR(devices.GetDefaultAudioEndpoint(0, 0, out device));
    var iid = typeof(IAudioEndpointVolume).GUID;
    object endpoint;
    Marshal.ThrowExceptionForHR(device.Activate(ref iid, 23, IntPtr.Zero, out endpoint));
    return (IAudioEndpointVolume)endpoint;
  }

  public static float Get() {
    float level;
    Marshal.ThrowExceptionForHR(Endpoint().GetMasterVolumeLevelScalar(out level));
    return level;
  }

  public static void Set(float level) {
    var context = Guid.Empty;
    Marshal.ThrowExceptionForHR(Endpoint().SetMasterVolumeLevelScalar(level, ref context));
  }
}
'@
$invariant = [Globalization.CultureInfo]::InvariantCulture
switch ($Action) {
  'get' { [Console]::Out.WriteLine([MoulinTestVolume]::Get().ToString($invariant)) }
  'set' { [MoulinTestVolume]::Set([float]$Value) }
  'watch' {
    $last = -1
    # The test stops the process. The limit stops a watcher that the test forgot.
    $end = (Get-Date).AddMinutes(10)
    while ((Get-Date) -lt $end) {
      $level = [Math]::Round([double][MoulinTestVolume]::Get(), 3)
      if ($level -ne $last) {
        [Console]::Out.WriteLine(('{0} {1}' -f [DateTimeOffset]::Now.ToUnixTimeMilliseconds(), $level.ToString($invariant)))
        [Console]::Out.Flush()
        $last = $level
      }
      Start-Sleep -Milliseconds 20
    }
  }
}
