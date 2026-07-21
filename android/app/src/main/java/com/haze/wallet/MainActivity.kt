package com.haze.wallet

import android.net.Uri
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.browser.customtabs.CustomTabColorSchemeParams
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.Lifecycle
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.haze.wallet.ui.theme.HazeNavItem
import com.haze.wallet.ui.theme.HazeGlassBottomBar
import com.haze.wallet.ui.theme.HazeTheme
import com.haze.wallet.ui.theme.LocalHazeColors
import com.haze.wallet.ui.theme.HazeCard
import com.haze.wallet.ui.theme.HazeScreenTitle
import kotlinx.coroutines.launch

// FragmentActivity (not plain ComponentActivity) - BiometricPrompt requires
// one to host its dialog.
class MainActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val repo = WalletRepository(SecureStorage(applicationContext))
        setContent {
            HazeTheme {
                Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    HazeApp(repo)
                }
            }
        }
    }
}

private fun openExplorer(context: android.content.Context, url: String, toolbarColor: Int) {
    if (url.isBlank()) return
    val params = CustomTabColorSchemeParams.Builder().setToolbarColor(toolbarColor).build()
    CustomTabsIntent.Builder()
        .setDefaultColorSchemeParams(params)
        .build()
        .launchUrl(context, Uri.parse(url))
}

@Composable
fun HazeApp(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    val navController = rememberNavController()
    val context = LocalContext.current
    val hazeColors = LocalHazeColors.current
    val surfaceColor = MaterialTheme.colorScheme.surface

    // Deliberately NOT gated directly on state.hasWallet: createWallet()
    // marks the wallet ready (and persists it) as soon as the keystore
    // exists, but the user still needs to see and confirm they've saved
    // the recovery phrase first. Gating this screen straight off
    // state.hasWallet meant the instant createWallet() flipped that flag,
    // this composable would react and unmount OnboardingFlow before the
    // "write it down" confirmation step ever got a chance to render - a
    // real bug (not a style issue) where every wallet creation silently
    // skipped its own seed-phrase confirmation. showOnboarding is local
    // and only flips once OnboardingFlow itself says it's done.
    var showOnboarding by remember { mutableStateOf(!state.hasWallet) }
    if (showOnboarding) {
        OnboardingFlow(repo, onDone = { showOnboarding = false })
        return
    }

    // Re-locks every time the app leaves the foreground (ON_STOP) - a
    // wallet holding real funds shouldn't stay unlocked just because the
    // process is still alive in the background. `unlocked` is plain
    // `remember`, not rememberSaveable, so a killed-and-restored process
    // starts locked too.
    var unlocked by remember { mutableStateOf(false) }
    val lifecycleOwner = LocalLifecycleOwner.current
    DisposableEffect(lifecycleOwner) {
        val observer = androidx.lifecycle.LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_STOP) unlocked = false
        }
        lifecycleOwner.lifecycle.addObserver(observer)
        onDispose { lifecycleOwner.lifecycle.removeObserver(observer) }
    }

    if (!unlocked) {
        LockScreen(onUnlocked = { unlocked = true })
        return
    }

    LaunchedEffect(Unit) {
        scope.launch { repo.refreshBalance() }
    }

    val navItems = remember(state.explorerUrl) {
        listOf(
            HazeNavItem("wallet", "Wallet", Icons.Filled.Home, route = "wallet"),
            HazeNavItem("send", "Send", Icons.Filled.Send, route = "send"),
            HazeNavItem("receive", "Receive", Icons.Filled.CallReceived, route = "receive"),
            HazeNavItem("history", "History", Icons.Filled.History, route = "history"),
            HazeNavItem(
                "explorer", "Explorer", Icons.Filled.Explore,
                onAction = {
                    if (state.explorerUrl.isBlank()) {
                        navController.navigate("more") { launchSingleTop = true }
                    } else {
                        openExplorer(context, state.explorerUrl, surfaceColor.toArgb())
                    }
                },
            ),
            HazeNavItem("more", "More", Icons.Filled.MoreHoriz, route = "more"),
        )
    }

    Box(
        modifier = Modifier.background(
            Brush.radialGradient(
                colors = listOf(hazeColors.glow1.copy(alpha = if (hazeColors.isDark) 0.5f else 0.7f), androidx.compose.ui.graphics.Color.Transparent),
                center = Offset(0.1f, 0f),
                radius = 900f,
            ),
        ),
    ) {
    com.haze.wallet.ui.theme.HazeAmbientBlobs()
    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        bottomBar = {
            val backStackEntry by navController.currentBackStackEntryAsState()
            val currentRoute = backStackEntry?.destination?.route
            HazeGlassBottomBar(
                items = navItems,
                currentRoute = currentRoute,
                onNavigate = { route -> navController.navigate(route) { launchSingleTop = true } },
            )
        }
    ) { padding ->
        NavHost(navController = navController, startDestination = "wallet", modifier = Modifier.padding(padding)) {
            composable("wallet") { WalletHomeScreen(repo, onNavigate = { route -> navController.navigate(route) { launchSingleTop = true } }) }
            composable("send") { SendScreen(repo) }
            composable("receive") { ReceiveScreen(repo) }
            composable("history") { HistoryScreen(repo) }
            composable("more") { MoreScreen(repo) }
        }
    }
    }
}

@Composable
private fun LockScreen(onUnlocked: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun tryUnlock() {
        val activity = context as? FragmentActivity ?: return
        busy = true
        scope.launch {
            val ok = BiometricLock.authenticate(activity, "Unlock Haze Wallet", "Confirm it's you before opening your wallet")
            busy = false
            if (ok) onUnlocked() else error = "Authentication cancelled - try again."
        }
    }

    LaunchedEffect(Unit) { tryUnlock() }

    Box(modifier = Modifier.fillMaxSize()) {
        com.haze.wallet.ui.theme.HazeAmbientBlobs()
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Icon(Icons.Filled.Lock, contentDescription = null, modifier = Modifier.size(40.dp), tint = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.height(16.dp))
            Text("Haze Wallet is locked", style = MaterialTheme.typography.headlineMedium)
            Spacer(Modifier.height(8.dp))
            Text(
                "Confirm your fingerprint, face, or device PIN to continue.",
                style = MaterialTheme.typography.bodyMedium,
                color = LocalHazeColors.current.inkFaint,
            )
            error?.let {
                Spacer(Modifier.height(12.dp))
                Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
            }
            Spacer(Modifier.height(24.dp))
            Button(onClick = { tryUnlock() }, enabled = !busy, modifier = Modifier.fillMaxWidth(0.7f)) {
                Text(if (busy) "Waiting…" else "Unlock")
            }
        }
    }
}

@Composable
private fun OnboardingFlow(repo: WalletRepository, onDone: () -> Unit) {
    var mode by remember { mutableStateOf("choose") } // choose | mnemonic | restore
    var generatedMnemonic by remember { mutableStateOf("") }
    var confirmed by remember { mutableStateOf(false) }
    var restorePhrase by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Haze Wallet", style = MaterialTheme.typography.headlineMedium)
        Spacer(Modifier.height(24.dp))

        when (mode) {
            "choose" -> {
                Text("Generates a real keystore (via Android's secure RNG), encrypted at rest by the Android Keystore.")
                Spacer(Modifier.height(16.dp))
                Button(onClick = {
                    scope.launch {
                        generatedMnemonic = repo.createWallet()
                        mode = "mnemonic"
                    }
                }, modifier = Modifier.fillMaxWidth()) { Text("Create Wallet") }
                Spacer(Modifier.height(8.dp))
                OutlinedButton(onClick = { mode = "restore" }, modifier = Modifier.fillMaxWidth()) {
                    Text("Restore from recovery phrase")
                }
            }
            "mnemonic" -> {
                Text("Save your recovery phrase", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(8.dp))
                Text("These 12 words are the ONLY way to recover this wallet. Anyone with this phrase can spend your funds - write it down and keep it private. Haze cannot recover it for you.")
                Spacer(Modifier.height(16.dp))
                HazeCard(modifier = Modifier.fillMaxWidth()) {
                    Text(generatedMnemonic)
                }
                Spacer(Modifier.height(16.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = confirmed, onCheckedChange = { confirmed = it })
                    Text("I've written down my recovery phrase")
                }
                Spacer(Modifier.height(16.dp))
                Button(
                    enabled = confirmed,
                    onClick = { onDone() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Continue") }
            }
            "restore" -> {
                Text("Restore from recovery phrase", style = MaterialTheme.typography.titleLarge)
                Spacer(Modifier.height(16.dp))
                OutlinedTextField(
                    value = restorePhrase,
                    onValueChange = { restorePhrase = it },
                    label = { Text("12-word recovery phrase") },
                    modifier = Modifier.fillMaxWidth(),
                )
                error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                Spacer(Modifier.height(16.dp))
                Button(onClick = {
                    scope.launch {
                        try {
                            repo.restoreWallet(restorePhrase)
                            onDone()
                        } catch (e: Exception) {
                            error = e.message ?: "restore failed"
                        }
                    }
                }, modifier = Modifier.fillMaxWidth()) { Text("Restore") }
            }
        }
    }
}

@Composable
private fun WalletHomeScreen(repo: WalletRepository, onNavigate: (String) -> Unit) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    var faucetMessage by remember { mutableStateOf<String?>(null) }

    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current

    // Keeps the node-status strip live, same "poll every few seconds"
    // feel the block explorer and homepage stats widget already use -
    // this screen is the one place a user can tell at a glance whether
    // their node is actually reachable.
    LaunchedEffect(Unit) {
        while (true) {
            repo.refreshNodeStatus()
            kotlinx.coroutines.delay(8000)
        }
    }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            com.haze.wallet.ui.theme.HazePulseDot(
                color = if (state.nodeOnline) MaterialTheme.colorScheme.tertiary else hazeColors.inkFaint,
            )
            Spacer(Modifier.width(6.dp))
            Text(
                state.claimedName?.let { "$it.haze" } ?: "Haze Wallet",
                style = MaterialTheme.typography.titleMedium,
                color = if (state.claimedName != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onBackground,
            )
        }
        Spacer(Modifier.height(16.dp))
        HazeCard(modifier = Modifier.fillMaxWidth(), padding = PaddingValues(20.dp)) {
            Column {
                Text("BALANCE", style = MaterialTheme.typography.labelSmall, color = hazeColors.inkFaint)
                Spacer(Modifier.height(6.dp))
                Text("${state.confirmedBalance}", style = MaterialTheme.typography.displaySmall)
                Spacer(Modifier.height(16.dp))
                Row {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("confirmed", style = MaterialTheme.typography.labelMedium, color = hazeColors.inkFaint)
                        Text("${state.confirmedBalance}", style = MaterialTheme.typography.titleSmall)
                    }
                    Column(modifier = Modifier.weight(1f)) {
                        Text("pending", style = MaterialTheme.typography.labelMedium, color = hazeColors.inkFaint)
                        Text("${state.pendingBalance}", style = MaterialTheme.typography.titleSmall, color = hazeColors.inkFaint)
                    }
                }
            }
        }
        Spacer(Modifier.height(20.dp))
        var faucetBusy by remember { mutableStateOf(false) }
        Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            com.haze.wallet.ui.theme.HazeQuickAction(label = "Send", icon = Icons.Filled.Send, primary = true) { onNavigate("send") }
            com.haze.wallet.ui.theme.HazeQuickAction(label = "Receive", icon = Icons.Filled.CallReceived) { onNavigate("receive") }
            com.haze.wallet.ui.theme.HazeQuickAction(label = "Faucet", icon = Icons.Filled.WaterDrop, enabled = !faucetBusy) {
                faucetBusy = true
                scope.launch {
                    try {
                        repo.claimFaucet(500)
                        faucetMessage = "Received 500. Refreshing balance…"
                    } catch (e: Exception) {
                        faucetMessage = "Faucet unavailable: ${e.message}"
                    }
                    faucetBusy = false
                }
            }
        }
        faucetMessage?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, style = MaterialTheme.typography.bodySmall, color = hazeColors.inkFaint)
        }

        Spacer(Modifier.height(20.dp))
        HazeCard(modifier = Modifier.fillMaxWidth(), padding = PaddingValues(vertical = 14.dp, horizontal = 4.dp)) {
            Row {
                StatCell(label = "HEIGHT", value = if (state.nodeOnline) "${state.nodeHeight}" else "—", modifier = Modifier.weight(1f))
                StatDivider()
                StatCell(label = "VALIDATORS", value = if (state.nodeOnline) "${state.nodeValidators}" else "—", modifier = Modifier.weight(1f))
                StatDivider()
                StatCell(label = "MEMPOOL", value = if (state.nodeOnline) "${state.nodeMempoolSize}" else "—", modifier = Modifier.weight(1f))
            }
        }

        Spacer(Modifier.height(24.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("recent activity", style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f))
            TextButton(onClick = { onNavigate("history") }) { Text("View all") }
        }
        if (state.activity.isEmpty()) {
            Spacer(Modifier.height(8.dp))
            Text("No activity yet.", style = MaterialTheme.typography.bodySmall, color = hazeColors.inkFaint)
        } else {
            Spacer(Modifier.height(4.dp))
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                state.activity.take(3).forEach { entry -> ActivityRow(entry) }
            }
        }
        Spacer(Modifier.height(20.dp))
        OutlinedButton(onClick = { scope.launch { repo.refreshBalance() } }, modifier = Modifier.fillMaxWidth()) {
            Text("Refresh balance")
        }
    }
}

@Composable
private fun StatCell(label: String, value: String, modifier: Modifier = Modifier) {
    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current
    Column(modifier = modifier.padding(horizontal = 12.dp)) {
        Text(label, style = MaterialTheme.typography.labelSmall, color = hazeColors.inkFaint)
        Spacer(Modifier.height(4.dp))
        Text(value, style = MaterialTheme.typography.labelMedium)
    }
}

@Composable
private fun StatDivider() {
    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current
    Box(modifier = Modifier.width(1.dp).height(28.dp).background(hazeColors.hairline))
}

/** Maps an activity title to a small icon glyph - Sent/Received/Claimed/
 * validator actions all read differently at a glance, mirroring the
 * liquid-glass mockup's per-row glyphs instead of a plain bullet list. */
private fun activityIcon(title: String): androidx.compose.ui.graphics.vector.ImageVector = when {
    title.startsWith("Received") -> Icons.Filled.CallReceived
    title.startsWith("Sent") || title.startsWith("Self-pay") -> Icons.Filled.Send
    title.startsWith("Claimed") || title.startsWith("Transferred") -> Icons.Filled.AlternateEmail
    title.startsWith("Registered") || title.startsWith("Recovered") -> Icons.Filled.Shield
    title.startsWith("Rotated") -> Icons.Filled.Autorenew
    title.startsWith("Restored") -> Icons.Filled.RestartAlt
    else -> Icons.Filled.History
}

@Composable
private fun ActivityRow(entry: ActivityEntry) {
    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current
    HazeCard(modifier = Modifier.fillMaxWidth(), padding = PaddingValues(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier.size(36.dp).background(hazeColors.fog2, androidx.compose.foundation.shape.RoundedCornerShape(12.dp)),
                contentAlignment = Alignment.Center,
            ) {
                Icon(activityIcon(entry.title), contentDescription = null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp))
            }
            Spacer(Modifier.width(12.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(entry.title, style = MaterialTheme.typography.titleSmall)
                if (entry.detail.isNotBlank()) Text(entry.detail, style = MaterialTheme.typography.bodySmall, color = hazeColors.inkFaint)
            }
        }
    }
}

@Composable
private fun SendScreen(repo: WalletRepository) {
    val scope = rememberCoroutineScope()
    var selfAmount by remember { mutableStateOf("") }
    var selfMessage by remember { mutableStateOf<String?>(null) }
    var selfBusy by remember { mutableStateOf(false) }
    var toName by remember { mutableStateOf("") }
    var nameAmount by remember { mutableStateOf("") }
    var nameMessage by remember { mutableStateOf<String?>(null) }
    var nameBusy by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        HazeScreenTitle("Send")
        Spacer(Modifier.height(24.dp))
        Text("Self-pay", style = MaterialTheme.typography.titleLarge)
        Text("Splits/consolidates your own confirmed UTXOs.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = selfAmount, onValueChange = { selfAmount = it },
            label = { Text("Amount") },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Button(
            enabled = !selfBusy && (selfAmount.toLongOrNull() ?: 0) > 0,
            onClick = {
                selfBusy = true
                scope.launch {
                    try {
                        repo.selfPay(selfAmount.toLongOrNull() ?: 0)
                        selfMessage = "Broadcast successfully. Balance will update once mined."
                        selfAmount = ""
                    } catch (e: Exception) {
                        selfMessage = e.message
                    }
                    selfBusy = false
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (selfBusy) "Sending…" else "Send") }
        selfMessage?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Send to a name", style = MaterialTheme.typography.titleLarge)
        Text("Sends directly to someone's registered .haze name.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = toName, onValueChange = { toName = it },
            label = { Text("Recipient's name") },
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = nameAmount, onValueChange = { nameAmount = it },
            label = { Text("Amount") },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Button(
            enabled = !nameBusy && toName.isNotBlank() && (nameAmount.toLongOrNull() ?: 0) > 0,
            onClick = {
                nameBusy = true
                nameMessage = "Sending…"
                scope.launch {
                    val err = repo.sendToName(toName.trim(), nameAmount.toLongOrNull() ?: 0)
                    nameMessage = err ?: "Sent."
                    if (err == null) { toName = ""; nameAmount = "" }
                    nameBusy = false
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (nameBusy) "Sending…" else "Send") }
        nameMessage?.let { Text(it) }
    }
}

@Composable
private fun ReceiveScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    var incomingSlate by remember { mutableStateOf("") }
    var responseOut by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var respondBusy by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        HazeScreenTitle("Receive")
        Spacer(Modifier.height(24.dp))
        Text("Your address", style = MaterialTheme.typography.titleLarge)
        Spacer(Modifier.height(8.dp))
        Text(state.claimedName?.let { "$it.haze" } ?: "Claim a name in the More tab to receive payments by name.")

        Spacer(Modifier.height(32.dp))
        Text("Receive a payment", style = MaterialTheme.typography.titleLarge)
        Text("Paste a slate someone sent you directly. This builds your response - send it back to them.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = incomingSlate,
            onValueChange = { incomingSlate = it },
            label = { Text("Incoming slate JSON") },
            modifier = Modifier.fillMaxWidth().height(160.dp),
        )
        Spacer(Modifier.height(8.dp))
        Button(
            enabled = !respondBusy && incomingSlate.isNotBlank(),
            onClick = {
                respondBusy = true
                scope.launch {
                    try {
                        responseOut = repo.respondToPastedSlate(incomingSlate)
                        error = null
                    } catch (e: Exception) {
                        error = e.message
                    }
                    respondBusy = false
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (respondBusy) "Responding…" else "Respond") }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        responseOut?.let {
            Spacer(Modifier.height(8.dp))
            Text("Send this back to the sender:")
            androidx.compose.foundation.text.selection.SelectionContainer { Text(it) }
        }
    }
}

@Composable
private fun HistoryScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current
    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        HazeScreenTitle("Activity")
        Spacer(Modifier.height(8.dp))
        Text("Everything this wallet has sent, received, or registered - newest first.", color = hazeColors.inkFaint)
        Spacer(Modifier.height(16.dp))
        if (state.activity.isEmpty()) {
            Text("No activity yet.", color = hazeColors.inkFaint)
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                state.activity.forEach { entry -> ActivityRow(entry) }
            }
        }
    }
}

@Composable
private fun MoreScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var nodeUrlField by remember { mutableStateOf(state.nodeUrl) }
    var explorerUrlField by remember { mutableStateOf(state.explorerUrl) }
    var stakeMinField by remember { mutableStateOf("1") }
    var stakeMessage by remember { mutableStateOf<String?>(null) }
    var revealedKey by remember { mutableStateOf<String?>(null) }
    var sweepKeyField by remember { mutableStateOf("") }
    var sweepMessage by remember { mutableStateOf<String?>(null) }

    var claimField by remember { mutableStateOf("") }
    var claimMessage by remember { mutableStateOf<String?>(null) }
    var claimBusy by remember { mutableStateOf(false) }
    var showNameTransfer by remember { mutableStateOf(false) }
    var transferName by remember { mutableStateOf("") }
    var transferTo by remember { mutableStateOf("") }
    var transferMessage by remember { mutableStateOf<String?>(null) }
    var transferBusy by remember { mutableStateOf(false) }
    var stakeBusy by remember { mutableStateOf(false) }
    var sweepBusy by remember { mutableStateOf(false) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        HazeScreenTitle("More")
        Spacer(Modifier.height(24.dp))

        Text(".haze name", style = MaterialTheme.typography.titleLarge)
        if (state.claimedName != null) {
            Text("You own ${state.claimedName}.haze - a one-time claim, nothing more to do here.", color = MaterialTheme.colorScheme.primary)
            Spacer(Modifier.height(8.dp))
            TextButton(onClick = { showNameTransfer = !showNameTransfer }) { Text(if (showNameTransfer) "Hide transfer" else "Transfer to someone else") }
            if (showNameTransfer) {
                OutlinedTextField(value = transferTo, onValueChange = { transferTo = it }, label = { Text("New owner pubkey (hex)") }, modifier = Modifier.fillMaxWidth())
                Spacer(Modifier.height(8.dp))
                Button(
                    enabled = !transferBusy && transferTo.isNotBlank(),
                    onClick = {
                        transferName = state.claimedName ?: return@Button
                        transferBusy = true
                        scope.launch {
                            transferMessage = repo.transferName(transferName, transferTo.trim()) ?: "Transferred."
                            transferBusy = false
                        }
                    },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(if (transferBusy) "Transferring…" else "Transfer") }
                transferMessage?.let { Text(it) }
            }
        } else {
            Text("Permanent, first-come-first-served, free to claim - sponsored by the network. You'll only need to do this once.")
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(value = claimField, onValueChange = { claimField = it }, label = { Text("Name") }, modifier = Modifier.fillMaxWidth())
            Spacer(Modifier.height(8.dp))
            Button(
                enabled = !claimBusy && claimField.isNotBlank(),
                onClick = {
                    claimBusy = true
                    scope.launch {
                        claimMessage = repo.claimName(claimField.trim()) ?: "Claiming - waiting for it to be mined…"
                        claimBusy = false
                    }
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(if (claimBusy) "Claiming…" else "Claim") }
            claimMessage?.let { Text(it) }
        }

        Spacer(Modifier.height(32.dp))
        Text("Node", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(value = nodeUrlField, onValueChange = { nodeUrlField = it }, label = { Text("Node URL") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(onClick = { repo.setNodeUrl(nodeUrlField.trim()) }, modifier = Modifier.fillMaxWidth()) { Text("Save node URL") }

        Spacer(Modifier.height(32.dp))
        Text("Block explorer", style = MaterialTheme.typography.titleLarge)
        Text("Set once, then reachable from the Explorer button in the bottom bar.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = explorerUrlField, onValueChange = { explorerUrlField = it },
            label = { Text("Explorer URL") },
            placeholder = { Text("https://…") },
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Button(onClick = { repo.setExplorerUrl(explorerUrlField.trim()) }, modifier = Modifier.fillMaxWidth()) { Text("Save explorer URL") }

        Spacer(Modifier.height(32.dp))
        Text("Become a validator", style = MaterialTheme.typography.titleLarge)
        Text("Stakes your single largest confirmed output. Doesn't spend anything - just registers ownership on-chain. To actually propose blocks, run your own node with the revealed key.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            value = stakeMinField, onValueChange = { stakeMinField = it },
            label = { Text("Minimum amount to stake") },
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(8.dp))
        Button(
            enabled = !stakeBusy,
            onClick = {
                stakeBusy = true
                scope.launch {
                    stakeMessage = repo.registerAsValidator(stakeMinField.toLongOrNull() ?: 1) ?: "Registered as a validator."
                    stakeBusy = false
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (stakeBusy) "Registering…" else "Register as validator") }
        stakeMessage?.let { Text(it) }
        Spacer(Modifier.height(8.dp))
        TextButton(onClick = {
            val activity = context as? FragmentActivity ?: return@TextButton
            scope.launch {
                if (BiometricLock.authenticate(activity, "Reveal validator key", "Confirm it's you before showing this key")) {
                    revealedKey = repo.revealStakeKey(stakeMinField.toLongOrNull() ?: 1)
                }
            }
        }) { Text("Reveal my validator key (to run my own node)") }
        revealedKey?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Recover validator rewards", style = MaterialTheme.typography.titleLarge)
        Text("If you've run your own node with a staked key, block rewards are sitting on-chain, provably yours. Sweep them in.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(value = sweepKeyField, onValueChange = { sweepKeyField = it }, label = { Text("Validator stake key (hex)") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(
            enabled = !sweepBusy && sweepKeyField.isNotBlank(),
            onClick = {
                sweepBusy = true
                scope.launch {
                    sweepMessage = repo.recoverValidatorRewards(sweepKeyField.trim()) ?: "Recovered rewards."
                    sweepBusy = false
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text(if (sweepBusy) "Recovering…" else "Recover rewards") }
        sweepMessage?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Rotate seed phrase", style = MaterialTheme.typography.titleLarge)
        Text("Generates a brand new recovery phrase and moves your entire confirmed balance to it in one on-chain transaction (a normal network fee applies). Your .haze name, if you have one, is transferred to the new phrase too - nothing else changes.")
        Spacer(Modifier.height(8.dp))
        var showRotateConfirm by remember { mutableStateOf(false) }
        var rotateMnemonic by remember { mutableStateOf<String?>(null) }
        var rotateNewKeystoreBytes by remember { mutableStateOf<ByteArray?>(null) }
        var rotateConfirmedSaved by remember { mutableStateOf(false) }
        var rotateBusy by remember { mutableStateOf(false) }
        var rotateMessage by remember { mutableStateOf<String?>(null) }

        if (rotateMnemonic == null) {
            OutlinedButton(
                onClick = {
                    rotateMessage = null
                    if (state.confirmedBalance <= 0) {
                        rotateMessage = "No confirmed balance to move - nothing to rotate yet."
                    } else {
                        showRotateConfirm = true
                    }
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Start") }
        } else {
            HazeCard(modifier = Modifier.fillMaxWidth()) {
                Text(rotateMnemonic!!)
            }
            Spacer(Modifier.height(8.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Checkbox(checked = rotateConfirmedSaved, onCheckedChange = { rotateConfirmedSaved = it })
                Text("I've written down the new recovery phrase")
            }
            Spacer(Modifier.height(8.dp))
            Button(
                enabled = rotateConfirmedSaved && !rotateBusy,
                onClick = {
                    val newBytes = rotateNewKeystoreBytes ?: return@Button
                    rotateBusy = true
                    scope.launch {
                        val result = repo.executeSeedRotation(newBytes)
                        rotateBusy = false
                        if (result == null) {
                            rotateMessage = "Done - your funds are moving to the new phrase now (confirms shortly)."
                            rotateMnemonic = null
                            rotateNewKeystoreBytes = null
                            rotateConfirmedSaved = false
                        } else {
                            rotateMessage = result
                        }
                    }
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text(if (rotateBusy) "Moving funds…" else "Move my funds to the new phrase") }
        }
        rotateMessage?.let { Text(it) }

        if (showRotateConfirm) {
            AlertDialog(
                onDismissRequest = { showRotateConfirm = false },
                title = { Text("Rotate seed phrase?") },
                text = { Text("This creates a new recovery phrase and moves your entire balance to it in one transaction. Your current phrase will no longer control these funds afterward.") },
                confirmButton = {
                    TextButton(onClick = {
                        showRotateConfirm = false
                        val activity = context as? FragmentActivity
                        scope.launch {
                            val ok = activity == null || BiometricLock.authenticate(activity, "Rotate seed phrase", "Confirm it's you before generating a new recovery phrase")
                            if (ok) {
                                val generated = repo.generateRotationCandidate()
                                rotateMnemonic = generated.mnemonic
                                rotateNewKeystoreBytes = generated.keystoreBytes
                            }
                        }
                    }) { Text("Continue") }
                },
                dismissButton = {
                    TextButton(onClick = { showRotateConfirm = false }) { Text("Cancel") }
                },
            )
        }

        Spacer(Modifier.height(32.dp))
        Button(
            onClick = { repo.lockWallet() },
            colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.error),
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Lock wallet") }
    }
}
