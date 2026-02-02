# Send - เครื่องมือย้ายไฟล์ข้ามเครื่องความเร็วสูง (High-Performance Local File Transfer)

<p align="center">
  <a href="#thai">🇹🇭 ภาษาไทย</a> •
  <a href="#english">🇺🇸 English</a>
</p>

โปรแกรมสำหรับส่งไฟล์จำนวนมากหรือโฟลเดอร์ขนาดใหญ่ผ่านเครือข่ายวงแลน (LAN) ถูกออกแบบมาเพื่อตอบโจทย์ **การย้ายข้อมูลไปคอมพิวเตอร์เครื่องใหม่** ที่ต้องการความเร็วสูงสุด ข้ามไฟล์เดิมได้ และสั่ง Resume ได้เมื่อหลุด

---

## <a id="thai"></a>🇹🇭 ภาษาไทย

### จุดเด่น (Features)
*   **Resume Capability**: สามารถหยุดและส่งต่อจากจุดเดิมได้ทันที ไม่ต้องเริ่มนับหนึ่งใหม่ (ข้ามไฟล์ที่ส่งเสร็จแล้ว เช็คเฉพาะไฟล์ที่ยังไม่เสร็จ)
*   **Performance Optimization**: ปรับแต่งมาเพื่อความเร็วสูงสุดสำหรับเครือข่าย LAN
    *   ใช้ **TCP_NODELAY** ลด Latency ในการส่งไฟล์เล็กๆ จำนวนมาก
    *   Buffer ขนาดใหญ่ **1MB** เพื่อการส่งไฟล์ใหญ่ที่ลื่นไหล
    *   Database แบบ **WAL Mode** เขียนสถานะไฟล์ได้รวดเร็ว ไม่คอขวดที่ Disk
*   **Status Tracking**: มีฐานข้อมูล (SQLite) เก็บสถานะทุกไฟล์ (Pending, Sent, Skipped)
*   **Smart ETA**: คำนวณเวลาที่เหลือจริง โดยดูจากขนาดไฟล์ที่ "เหลือต้องส่ง" เท่านั้น

### คำเตือน (Warning) ⚠️
โปรแกรมนี้เน้นความสะดวกและความเร็วเป็นหลัก **จึงไม่มีระบบรักษาความปลอดภัย**
*   ❌ **ไม่มี Login/Password**: ใครก็สามารถเชื่อมต่อเข้า Server ได้
*   ❌ **ไม่มีการกด Confirm**: ฝั่ง Server จะรับไฟล์ทันทีที่ส่งมา ลงในโฟลเดอร์ที่กำหนด
*   ✅ **คำแนะนำ**: ควรใช้ในเครือข่ายส่วนตัว (Private LAN) ภายในบ้านหรือออฟฟิศที่เชื่อถือได้เท่านั้น **ห้ามเปิด Port ออก Public Internet เด็ดขาด**

### วิธีใช้งาน (Usage)

#### 1. ฝั่งเครื่องรับ (Server)
รันคำสั่งเพื่อรอรับไฟล์ โดยระบุโฟลเดอร์ปลายทางและ Port
```bash
# รูปแบบ: send serve <โฟลเดอร์เก็บไฟล์> <Port>
send serve "D:\BackupTarget" 8080
```

#### 2. ฝั่งเครื่องส่ง (Client)
**เริ่มส่งไฟล์ใหม่ (Push):**
```bash
# รูปแบบ: send push <โฟลเดอร์ต้นทาง> <IP เครื่องรับ> <Port>
send push "C:\MyWork" 192.168.1.50 8080
```

**ดูรายการที่เคยส่ง (List):**
```bash
send list
```

**ส่งต่อจากจุดเดิม (Resume):**
หากการส่งหยุดชะงัก หรือปิดโปรแกรมไป สามารถ Resume ได้ด้วย ID (ดู ID จากคำสั่ง list)
```bash
send resume 1
```

**เริ่มส่งใหม่ทั้งหมด (Restart):**
หากต้องการ Reset สถานะและบังคับส่งใหม่ทั้งหมดของ ID นั้น
```bash
send restart 1
```

**ลบประวัติการส่ง (Remove):**
ลบประวัติและไฟล์ log ของ ID นั้น (ต้องยืนยันการลบ)
```bash
send remove 1
```

---

## <a id="english"></a>🇺🇸 English

### Purpose
**Send** is a CLI tool designed primarily for **migrating data to a new machine**. It handles massive amounts of files efficiently over a Local Area Network (LAN). It solves the pain point of Windows File Sharing or standard copies failing in the middle and needing a full restart.

### Key Features
*   **Robust Resume**: Stop and resume transfers anytime. It intelligently skips completed files and only re-transmits what's pending or incomplete.
*   **High Performance**: Tuned for maximum throughput on LAN.
    *   **TCP_NODELAY** enabled for low latency on small files.
    *   **1MB Buffer** for efficient large file streaming.
    *   **WAL Mode Database** for high-speed logging of file statuses.
*   **Real-time Progress**: Shows current file, speed, transfer stats, and accurate ETA based on remaining pending data.

### Security Warning ⚠️
This tool prioritizes speed and ease of use over security.
*   ❌ **No Authentication**: No username or password required.
*   ❌ **No Approval**: The server automatically accepts all incoming files to the designated folder.
*   ✅ **Recommendation**: Use this ONLY on a **Safe, Private LAN** (Home/Office). **NEVER expose the listening port to the Public Internet.**

### Usage

#### 1. Receiver (Server)
Start the server listening on a specific folder and port.
```bash
# Usage: send serve <TargetFolder> <Port>
send serve "D:\BackupTarget" 8080
```

#### 2. Sender (Client)
**Start a new transfer (Push):**
```bash
# Usage: send push <SourceFolder> <ReceiverIP> <Port>
send push "C:\MyWork" 192.168.1.50 8080
```

**List transfer history (List):**
```bash
send list
```

**Resume a transfer (Resume):**
Pick up where you left off using the Transfer ID (find it using `list`).
```bash
send resume 1
```

**Force Restart (Restart):**
Clear the progress log and re-send everything for a specific Transfer ID.
```bash
send restart 1
```

**Remove History (Remove):**
Delete transfer history and log files for a specific Transfer ID.
```bash
send remove 1
```
