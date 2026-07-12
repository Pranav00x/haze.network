package com.haze.wallet

import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.browser.customtabs.CustomTabColorSchemeParams
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import com.haze.wallet.ui.theme.HazeNavItem
import com.haze.wallet.ui.theme.HazeGlassBottomBar
import com.haze.wallet.ui.theme.HazeTheme
import com.haze.wallet.ui.theme.LocalHazeColors
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
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

    if (!state.hasWallet) {
        OnboardingFlow(repo)
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
            HazeNavItem("names", "Names", Icons.Filled.AlternateEmail, route = "names"),
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

    Scaffold(
        containerColor = androidx.compose.ui.graphics.Color.Transparent,
        modifier = Modifier.background(
            Brush.radialGradient(
                colors = listOf(hazeColors.glow1.copy(alpha = if (hazeColors.isDark) 0.5f else 0.7f), androidx.compose.ui.graphics.Color.Transparent),
                center = Offset(0.1f, 0f),
                radius = 900f,
            ),
        ),
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
            composable("wallet") { WalletHomeScreen(repo) }
            composable("send") { SendScreen(repo) }
            composable("receive") { ReceiveScreen(repo) }
            composable("names") { NamesScreen(repo) }
            composable("history") { HistoryScreen(repo) }
            composable("more") { MoreScreen(repo) }
        }
    }
}

@Composable
private fun OnboardingFlow(repo: WalletRepository) {
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
                Card(modifier = Modifier.fillMaxWidth()) {
                    Text(generatedMnemonic, modifier = Modifier.padding(16.dp))
                }
                Spacer(Modifier.height(16.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(checked = confirmed, onCheckedChange = { confirmed = it })
                    Text("I've written down my recovery phrase")
                }
                Spacer(Modifier.height(16.dp))
                Button(
                    enabled = confirmed,
                    onClick = { mode = "done" },
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
private fun WalletHomeScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    var faucetMessage by remember { mutableStateOf<String?>(null) }

    val hazeColors = com.haze.wallet.ui.theme.LocalHazeColors.current

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Text(
            state.claimedName?.let { "$it.haze" } ?: "Haze Wallet",
            style = MaterialTheme.typography.titleMedium,
            color = if (state.claimedName != null) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onBackground,
        )
        Spacer(Modifier.height(16.dp))
        Surface(
            shape = RoundedCornerShape(20.dp),
            color = hazeColors.cardVeil.copy(alpha = if (hazeColors.isDark) 0.5f else 0.7f),
            border = BorderStroke(1.dp, hazeColors.hairline),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(modifier = Modifier.padding(20.dp)) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("CONFIRMED", style = MaterialTheme.typography.labelSmall, color = hazeColors.inkFaint)
                    Spacer(Modifier.height(4.dp))
                    Text("${state.confirmedBalance}", style = MaterialTheme.typography.displaySmall)
                }
                Column(modifier = Modifier.weight(1f)) {
                    Text("PENDING", style = MaterialTheme.typography.labelSmall, color = hazeColors.inkFaint)
                    Spacer(Modifier.height(4.dp))
                    Text("${state.pendingBalance}", style = MaterialTheme.typography.displaySmall, color = hazeColors.inkFaint)
                }
            }
        }
        Spacer(Modifier.height(24.dp))
        Button(
            onClick = {
                scope.launch {
                    try {
                        repo.claimFaucet(500)
                        faucetMessage = "Received 500. Refreshing balance…"
                    } catch (e: Exception) {
                        faucetMessage = "Faucet unavailable: ${e.message}"
                    }
                }
            },
            modifier = Modifier.fillMaxWidth(),
        ) { Text("Get devnet funds") }
        faucetMessage?.let {
            Spacer(Modifier.height(8.dp))
            Text(it)
        }
        Spacer(Modifier.height(16.dp))
        OutlinedButton(onClick = { scope.launch { repo.refreshBalance() } }, modifier = Modifier.fillMaxWidth()) {
            Text("Refresh balance")
        }
    }
}

@Composable
private fun SendScreen(repo: WalletRepository) {
    val scope = rememberCoroutineScope()
    var selfAmount by remember { mutableStateOf("") }
    var selfMessage by remember { mutableStateOf<String?>(null) }
    var toName by remember { mutableStateOf("") }
    var nameAmount by remember { mutableStateOf("") }
    var nameMessage by remember { mutableStateOf<String?>(null) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
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
        Button(onClick = {
            scope.launch {
                try {
                    repo.selfPay(selfAmount.toLongOrNull() ?: 0)
                    selfMessage = "Broadcast successfully. Balance will update once mined."
                } catch (e: Exception) {
                    selfMessage = e.message
                }
            }
        }, modifier = Modifier.fillMaxWidth()) { Text("Send") }
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
        Button(onClick = {
            scope.launch {
                nameMessage = "Sending…"
                val err = repo.sendToName(toName.trim(), nameAmount.toLongOrNull() ?: 0)
                nameMessage = err ?: "Sent."
            }
        }, modifier = Modifier.fillMaxWidth()) { Text("Send") }
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

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Text("Your address", style = MaterialTheme.typography.titleLarge)
        Spacer(Modifier.height(8.dp))
        Text(state.claimedName?.let { "$it.haze" } ?: "Claim a name in the Names tab to receive payments by name.")

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
        Button(onClick = {
            scope.launch {
                try {
                    responseOut = repo.respondToPastedSlate(incomingSlate)
                    error = null
                } catch (e: Exception) {
                    error = e.message
                }
            }
        }, modifier = Modifier.fillMaxWidth()) { Text("Respond") }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        responseOut?.let {
            Spacer(Modifier.height(8.dp))
            Text("Send this back to the sender:")
            androidx.compose.foundation.text.selection.SelectionContainer { Text(it) }
        }
    }
}

@Composable
private fun NamesScreen(repo: WalletRepository) {
    val scope = rememberCoroutineScope()
    var claimField by remember { mutableStateOf("") }
    var claimMessage by remember { mutableStateOf<String?>(null) }
    var lookupField by remember { mutableStateOf("") }
    var lookupResult by remember { mutableStateOf<String?>(null) }
    var transferName by remember { mutableStateOf("") }
    var transferTo by remember { mutableStateOf("") }
    var transferMessage by remember { mutableStateOf<String?>(null) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Text("Claim a .haze name", style = MaterialTheme.typography.titleLarge)
        Text("Permanent, first-come-first-served. Free to claim - sponsored by the network.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(value = claimField, onValueChange = { claimField = it }, label = { Text("Name") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(onClick = {
            scope.launch { claimMessage = repo.claimName(claimField.trim()) ?: "Claiming - waiting for it to be mined…" }
        }, modifier = Modifier.fillMaxWidth()) { Text("Claim") }
        claimMessage?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Look up a name", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(value = lookupField, onValueChange = { lookupField = it }, label = { Text("Name") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(onClick = {
            scope.launch { lookupResult = repo.lookupName(lookupField.trim())?.toString() ?: "not registered" }
        }, modifier = Modifier.fillMaxWidth()) { Text("Look up") }
        lookupResult?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Transfer a name you own", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(value = transferName, onValueChange = { transferName = it }, label = { Text("Name") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(value = transferTo, onValueChange = { transferTo = it }, label = { Text("New owner pubkey (hex)") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(onClick = {
            scope.launch { transferMessage = repo.transferName(transferName.trim(), transferTo.trim()) ?: "Transferred." }
        }, modifier = Modifier.fillMaxWidth()) { Text("Transfer") }
        transferMessage?.let { Text(it) }
    }
}

@Composable
private fun HistoryScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
        Text("Activity", style = MaterialTheme.typography.titleLarge)
        Text("Everything this wallet has sent, received, or registered - newest first.")
        Spacer(Modifier.height(16.dp))
        if (state.activity.isEmpty()) {
            Text("No activity yet.")
        } else {
            state.activity.forEach { entry ->
                Card(modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp)) {
                    Column(modifier = Modifier.padding(12.dp)) {
                        Text(entry.title, style = MaterialTheme.typography.titleSmall)
                        if (entry.detail.isNotBlank()) Text(entry.detail, style = MaterialTheme.typography.bodySmall)
                    }
                }
            }
        }
    }
}

@Composable
private fun MoreScreen(repo: WalletRepository) {
    val state by repo.state.collectAsState()
    val scope = rememberCoroutineScope()
    var nodeUrlField by remember { mutableStateOf(state.nodeUrl) }
    var explorerUrlField by remember { mutableStateOf(state.explorerUrl) }
    var stakeMinField by remember { mutableStateOf("1") }
    var stakeMessage by remember { mutableStateOf<String?>(null) }
    var revealedKey by remember { mutableStateOf<String?>(null) }
    var sweepKeyField by remember { mutableStateOf("") }
    var sweepMessage by remember { mutableStateOf<String?>(null) }

    Column(modifier = Modifier.fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState())) {
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
        Button(onClick = {
            scope.launch { stakeMessage = repo.registerAsValidator(stakeMinField.toLongOrNull() ?: 1) ?: "Registered as a validator." }
        }, modifier = Modifier.fillMaxWidth()) { Text("Register as validator") }
        stakeMessage?.let { Text(it) }
        Spacer(Modifier.height(8.dp))
        TextButton(onClick = {
            scope.launch { revealedKey = repo.revealStakeKey(stakeMinField.toLongOrNull() ?: 1) }
        }) { Text("Reveal my validator key (to run my own node)") }
        revealedKey?.let { Text(it) }

        Spacer(Modifier.height(32.dp))
        Text("Recover validator rewards", style = MaterialTheme.typography.titleLarge)
        Text("If you've run your own node with a staked key, block rewards are sitting on-chain, provably yours. Sweep them in.")
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(value = sweepKeyField, onValueChange = { sweepKeyField = it }, label = { Text("Validator stake key (hex)") }, modifier = Modifier.fillMaxWidth())
        Spacer(Modifier.height(8.dp))
        Button(onClick = {
            scope.launch { sweepMessage = repo.recoverValidatorRewards(sweepKeyField.trim()) ?: "Recovered rewards." }
        }, modifier = Modifier.fillMaxWidth()) { Text("Recover rewards") }
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
            Card(modifier = Modifier.fillMaxWidth()) {
                Text(rotateMnemonic!!, modifier = Modifier.padding(16.dp))
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
                        val generated = repo.generateRotationCandidate()
                        rotateMnemonic = generated.mnemonic
                        rotateNewKeystoreBytes = generated.keystoreBytes
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
